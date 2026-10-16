//! Structural and normalized rendering budgets for the static SVG profile.
use crate::{ImageError, MAX_SVG_NODES};
use resvg::{tiny_skia::Transform, usvg};
use std::collections::{HashMap, VecDeque};

const MAX_DEPTH: usize = 64;
const MAX_EXPANDED_NODES: usize = 16384;
const MAX_EDGES: usize = 16384;
const MAX_ISOLATED_GROUPS: usize = 8;
const MAX_SCRATCH_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PIXEL_WORK: u64 = 128 * 1024 * 1024;
const MAX_SEGMENTS: usize = 65536;

pub(crate) fn validate_source(text: &str) -> Result<(), ImageError> {
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_SVG_NODES as u32,
            ..Default::default()
        },
    )
    .map_err(|e| ImageError::Invalid(e.to_string()))?;
    if document.root_element().tag_name().name() != "svg" {
        return Err(ImageError::Invalid("expected svg root".into()));
    }
    let nodes: Vec<_> = document
        .descendants()
        .filter(|node| node.is_element())
        .collect();
    let positions: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id(), i))
        .collect();
    let mut ids = HashMap::new();
    let mut refs = vec![Vec::<String>::new(); nodes.len()];
    let mut attribute_bytes = 0usize;
    for (i, node) in nodes.iter().enumerate() {
        if node.tag_name().name().len() > 256 {
            return Err(ImageError::Limit("SVG element name exceeds 256 bytes"));
        }
        if node.ancestors().count() > MAX_DEPTH {
            return Err(ImageError::Limit("SVG depth exceeds 64"));
        }
        if node
            .tag_name()
            .namespace()
            .is_some_and(|ns| ns != "http://www.w3.org/2000/svg")
        {
            return Err(ImageError::Unsupported("SVG element namespace".into()));
        }
        if !matches!(
            node.tag_name().name(),
            "svg"
                | "g"
                | "defs"
                | "path"
                | "rect"
                | "circle"
                | "ellipse"
                | "line"
                | "polyline"
                | "polygon"
                | "linearGradient"
                | "radialGradient"
                | "stop"
                | "clipPath"
                | "mask"
                | "title"
                | "desc"
        ) {
            return Err(ImageError::Unsupported(format!(
                "SVG element {}",
                node.tag_name().name()
            )));
        }
        if let Some(id) = node.attribute("id")
            && (id.is_empty() || ids.insert(id, i).is_some())
        {
            return Err(ImageError::Invalid("empty or duplicate SVG id".into()));
        }
        for attribute in node.attributes() {
            if attribute.name().len() > 256 {
                return Err(ImageError::Limit("SVG attribute name exceeds 256 bytes"));
            }
            attribute_bytes += attribute.value().len();
            if attribute.value().len() > 65536 || attribute_bytes > 1024 * 1024 {
                return Err(ImageError::Limit(
                    "SVG attributes exceed 64 KiB each / 1 MiB total",
                ));
            }
            let value = attribute.value();
            let lower = value.to_ascii_lowercase();
            if attribute.name().to_ascii_lowercase().starts_with("on")
                || attribute.name() == "filter"
                || lower.contains('@')
                || lower.contains("var(")
                || lower.contains('\\')
                || lower.contains("/*")
                || lower.contains("*/")
            {
                return Err(ImageError::Unsupported(
                    "SVG external, active or escaped attribute".into(),
                ));
            }
            if attribute.name() == "style" {
                validate_style(value)?;
            }
            if attribute.name() == "href" {
                if !matches!(node.tag_name().name(), "linearGradient" | "radialGradient") {
                    return Err(ImageError::Unsupported(
                        "SVG href is limited to gradients".into(),
                    ));
                }
                refs[i].push(internal_reference(value)?.to_owned());
            }
            let mut tail = value;
            let mut lower_tail = lower.as_str();
            while let Some(start) = lower_tail.find("url(") {
                let contents = &tail[start + 4..];
                let end = contents
                    .find(')')
                    .ok_or_else(|| ImageError::Invalid("malformed SVG URL".into()))?;
                let reference = contents[..end].trim().trim_matches(['\'', '"']);
                refs[i].push(internal_reference(reference)?.to_owned());
                let advance = start + 4 + end + 1;
                tail = &tail[advance..];
                lower_tail = &lower_tail[advance..];
                if refs[i].len() > MAX_EDGES {
                    return Err(ImageError::Limit("SVG references"));
                }
            }
        }
    }
    let mut graph = vec![Vec::new(); nodes.len()];
    let mut indegree = vec![0usize; nodes.len()];
    let mut edges = 0;
    for (i, node) in nodes.iter().enumerate() {
        graph[i].extend(
            node.children()
                .filter_map(|child| positions.get(&child.id()).copied()),
        );
        for reference in &refs[i] {
            graph[i].push(*ids.get(reference.as_str()).ok_or_else(|| {
                ImageError::Invalid(format!("missing SVG reference #{reference}"))
            })?);
        }
        edges += graph[i].len();
        if edges > MAX_EDGES {
            return Err(ImageError::Limit("SVG references"));
        }
        for &next in &graph[i] {
            indegree[next] += 1;
        }
    }
    // Both containment and resource references belong to the graph: a mask
    // which references itself through a child is cyclic too. Count expansion
    // before usvg has an opportunity to clone referenced subtrees.
    let mut queue: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    let mut order = Vec::new();
    let mut depths = vec![1usize; nodes.len()];
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &next in &graph[i] {
            depths[next] = depths[next].max(depths[i] + 1);
            if depths[next] > MAX_DEPTH {
                return Err(ImageError::Limit("SVG expanded reference depth exceeds 64"));
            }
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if order.len() != nodes.len() {
        return Err(ImageError::Invalid("cyclic SVG resource reference".into()));
    }
    let mut expanded = vec![1usize; nodes.len()];
    for &i in order.iter().rev() {
        for &next in &graph[i] {
            expanded[i] = expanded[i].saturating_add(expanded[next]);
        }
        if expanded[i] > MAX_EXPANDED_NODES {
            return Err(ImageError::Limit(
                "SVG expanded references exceed 16384 nodes",
            ));
        }
    }
    Ok(())
}

fn internal_reference(value: &str) -> Result<&str, ImageError> {
    let id = value
        .strip_prefix('#')
        .filter(|id| !id.is_empty() && !id.chars().any(char::is_whitespace));
    id.ok_or_else(|| ImageError::Unsupported("SVG reference must be a local #id".into()))
}
fn validate_style(style: &str) -> Result<(), ImageError> {
    for declaration in style.split(';').filter(|s| !s.trim().is_empty()) {
        let (property, _) = declaration
            .split_once(':')
            .ok_or_else(|| ImageError::Invalid("malformed SVG style declaration".into()))?;
        if !matches!(
            property.trim().to_ascii_lowercase().as_str(),
            "fill"
                | "fill-opacity"
                | "fill-rule"
                | "stroke"
                | "stroke-opacity"
                | "stroke-width"
                | "stroke-linecap"
                | "stroke-linejoin"
                | "stroke-miterlimit"
                | "stroke-dasharray"
                | "stroke-dashoffset"
                | "opacity"
                | "clip-path"
                | "clip-rule"
                | "mask"
                | "mask-type"
                | "color"
                | "stop-color"
                | "stop-opacity"
                | "display"
                | "visibility"
                | "shape-rendering"
                | "color-interpolation"
                | "paint-order"
                | "isolation"
                | "mix-blend-mode"
        ) {
            return Err(ImageError::Unsupported(format!(
                "SVG style property {}",
                property.trim()
            )));
        }
    }
    Ok(())
}

#[derive(Default)]
struct Budget {
    nodes: usize,
    isolated: usize,
    segments: usize,
    scratch: u64,
    work: u64,
}
impl Budget {
    fn visit(&mut self, depth: usize) -> Result<(), ImageError> {
        self.nodes += 1;
        if self.nodes > MAX_EXPANDED_NODES || depth > MAX_DEPTH {
            return Err(ImageError::Limit("SVG normalized tree traversal"));
        }
        Ok(())
    }
    fn allocation(&mut self, size: [u32; 2], bytes_per_pixel: u64) -> Result<(), ImageError> {
        self.scratch = self
            .scratch
            .saturating_add(u64::from(size[0]) * u64::from(size[1]) * bytes_per_pixel);
        if self.scratch > MAX_SCRATCH_BYTES {
            return Err(ImageError::Limit(
                "SVG aggregate compositing allocations exceed 128 MiB",
            ));
        }
        Ok(())
    }
    fn pixels(&mut self, size: [u32; 2], passes: u64) -> Result<(), ImageError> {
        self.work = self
            .work
            .saturating_add(u64::from(size[0]) * u64::from(size[1]) * passes);
        if self.work > MAX_PIXEL_WORK {
            return Err(ImageError::Limit("SVG raster work exceeds 128 Mi pixels"));
        }
        Ok(())
    }
}

/// Bounding-box extent independent of translation. Added padding covers both
/// raster AA and integer rounding. resvg clamps every layer to a canvas with
/// five times the output width and height; cap each axis at that actual limit.
fn extent(
    width: f32,
    height: f32,
    transform: Transform,
    cap: [u32; 2],
    origin: [f32; 2],
) -> Result<[u32; 2], ImageError> {
    let w = f64::from(width) * f64::from(transform.sx).abs()
        + f64::from(height) * f64::from(transform.kx).abs();
    let h = f64::from(width) * f64::from(transform.ky).abs()
        + f64::from(height) * f64::from(transform.sy).abs();
    // Large translated coordinates can inflate the f32 corner difference.
    // Include a conservative operation-rounding allowance before axis caps.
    let x = f64::from(origin[0]).abs() + f64::from(width);
    let y = f64::from(origin[1]).abs() + f64::from(height);
    let rounding = |a: f32, b: f32, t: f32| {
        (x * f64::from(a).abs() + y * f64::from(b).abs() + f64::from(t).abs())
            * f64::from(f32::EPSILON)
            * 32.0
    };
    let w = w + rounding(transform.sx, transform.kx, transform.tx);
    let h = h + rounding(transform.ky, transform.sy, transform.ty);
    if !w.is_finite() || !h.is_finite() {
        return Err(ImageError::Limit("SVG transformed bounds overflow"));
    }
    Ok([
        (w.ceil() + 8.0).min(f64::from(cap[0])) as u32,
        (h.ceil() + 8.0).min(f64::from(cap[1])) as u32,
    ])
}

pub(crate) fn validate_render(
    tree: &usvg::Tree,
    width: u32,
    height: u32,
    scale: f32,
) -> Result<(), ImageError> {
    let mut budget = Budget::default();
    let canvas = [width, height];
    let cap = [width * 5, height * 5];
    budget.allocation(canvas, 4)?;
    visit_children(
        tree.root(),
        Transform::from_scale(scale, scale),
        canvas,
        cap,
        0,
        &mut budget,
    )
}
fn visit_children(
    group: &usvg::Group,
    transform: Transform,
    canvas: [u32; 2],
    cap: [u32; 2],
    depth: usize,
    budget: &mut Budget,
) -> Result<(), ImageError> {
    budget.visit(depth)?;
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => {
                let transform = transform.pre_concat(child.transform());
                let mut child_canvas = canvas;
                if child.should_isolate() {
                    budget.isolated += 1;
                    if budget.isolated > MAX_ISOLATED_GROUPS {
                        return Err(ImageError::Limit(
                            "SVG exceeds eight actual isolated group uses",
                        ));
                    }
                    if !child.filters().is_empty() {
                        return Err(ImageError::Unsupported("SVG filters".into()));
                    }
                    let bounds = child.layer_bounding_box();
                    child_canvas = extent(
                        bounds.width(),
                        bounds.height(),
                        transform,
                        cap,
                        [bounds.x(), bounds.y()],
                    )?;
                    budget.allocation(child_canvas, 4)?;
                    budget.pixels(child_canvas, 1)?;
                }
                visit_children(child, transform, child_canvas, cap, depth + 1, budget)?;
                if let Some(clip) = child.clip_path() {
                    visit_clip(clip, transform, child_canvas, cap, depth + 1, budget)?;
                }
                if let Some(mask) = child.mask() {
                    visit_mask(mask, transform, child_canvas, cap, depth + 1, budget)?;
                }
            }
            usvg::Node::Path(path) => {
                budget.visit(depth + 1)?;
                budget.segments = budget
                    .segments
                    .saturating_add(path.data().segments().count());
                if budget.segments > MAX_SEGMENTS {
                    return Err(ImageError::Limit("SVG expanded path segments exceed 65536"));
                }
                let bounds = path.stroke_bounding_box();
                let area = extent(
                    bounds.width(),
                    bounds.height(),
                    transform,
                    canvas,
                    [bounds.x(), bounds.y()],
                )?;
                budget.pixels(area, 2)?;
                if let Some(stroke) = path.stroke() {
                    validate_dash(path, stroke, budget)?;
                }
            }
            _ => return Err(ImageError::Unsupported("SVG normalized image/text".into())),
        }
    }
    Ok(())
}
fn visit_clip(
    clip: &usvg::ClipPath,
    transform: Transform,
    canvas: [u32; 2],
    cap: [u32; 2],
    depth: usize,
    budget: &mut Budget,
) -> Result<(), ImageError> {
    budget.visit(depth)?;
    budget.allocation(canvas, 5)?;
    budget.pixels(canvas, 2)?;
    // Generic traversal conservatively counts nested isolated clip groups;
    // clip rendering can allocate a full parent canvas for those groups.
    visit_clip_children(
        clip.root(),
        transform.pre_concat(clip.transform()),
        canvas,
        cap,
        depth + 1,
        budget,
    )?;
    if let Some(next) = clip.clip_path() {
        visit_clip(next, transform, canvas, cap, depth + 1, budget)?;
    }
    Ok(())
}
fn visit_clip_children(
    group: &usvg::Group,
    transform: Transform,
    canvas: [u32; 2],
    cap: [u32; 2],
    depth: usize,
    budget: &mut Budget,
) -> Result<(), ImageError> {
    budget.visit(depth)?;
    for node in group.children() {
        if let usvg::Node::Group(child) = node {
            let transform = transform.pre_concat(child.transform());
            if let Some(clip) = child.clip_path() {
                budget.allocation(canvas, 4)?;
                budget.pixels(canvas, 1)?;
                visit_clip(clip, transform, canvas, cap, depth + 1, budget)?;
            }
            visit_clip_children(child, transform, canvas, cap, depth + 1, budget)?;
        } else if let usvg::Node::Path(path) = node {
            budget.visit(depth + 1)?;
            budget.segments = budget
                .segments
                .saturating_add(path.data().segments().count());
            if budget.segments > MAX_SEGMENTS {
                return Err(ImageError::Limit("SVG expanded path segments exceed 65536"));
            }
            let bounds = path.bounding_box();
            budget.pixels(
                extent(
                    bounds.width(),
                    bounds.height(),
                    transform,
                    canvas,
                    [bounds.x(), bounds.y()],
                )?,
                1,
            )?;
        } else {
            return Err(ImageError::Unsupported(
                "SVG normalized clip image/text".into(),
            ));
        }
    }
    Ok(())
}
fn visit_mask(
    mask: &usvg::Mask,
    transform: Transform,
    canvas: [u32; 2],
    cap: [u32; 2],
    depth: usize,
    budget: &mut Budget,
) -> Result<(), ImageError> {
    budget.visit(depth)?;
    budget.allocation(canvas, 6)?;
    budget.pixels(canvas, 3)?;
    visit_children(mask.root(), transform, canvas, cap, depth + 1, budget)?;
    if let Some(next) = mask.mask() {
        visit_mask(next, transform, canvas, cap, depth + 1, budget)?;
    }
    Ok(())
}

fn validate_dash(
    path: &usvg::Path,
    stroke: &usvg::Stroke,
    budget: &mut Budget,
) -> Result<(), ImageError> {
    let Some(dashes) = stroke.dasharray() else {
        return Ok(());
    };
    let interval: f64 = dashes.iter().map(|n| f64::from(*n)).sum();
    if interval <= 0.0 {
        return Err(ImageError::Invalid("SVG dash interval".into()));
    }
    // A Bezier's control polygon bounds its arclength; a closed contour adds
    // at most the same polygon length. This bounds dash expansion before raster.
    let mut distance = 0.0;
    let mut previous: Option<(f64, f64)> = None;
    for point in path.data().points() {
        if let Some((x, y)) = previous {
            distance += (f64::from(point.x) - x).hypot(f64::from(point.y) - y);
        }
        previous = Some((f64::from(point.x), f64::from(point.y)));
    }
    let count = (distance * 2.0 / interval * dashes.len() as f64).ceil();
    if !count.is_finite() || count > MAX_SEGMENTS as f64 {
        return Err(ImageError::Limit("SVG dash expansion exceeds 65536"));
    }
    budget.segments = budget.segments.saturating_add(count as usize);
    if budget.segments > MAX_SEGMENTS {
        return Err(ImageError::Limit(
            "SVG total path/dash expansion exceeds 65536",
        ));
    }
    Ok(())
}
