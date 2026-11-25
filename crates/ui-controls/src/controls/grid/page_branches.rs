//! Independent provider windows for lazily expanded group paths.
use super::*;
use rust_desktop_ui_data::{
    DataLimits, PageCacheSettings, PageCompletionSender, PageRequest, PagedDataSource,
    QueryCapabilities, QueryRequest, TableSchema,
};
use std::collections::BTreeMap;
type PathKey = Vec<(String, String)>;
type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;
struct Branch {
    host: WidgetId,
    page: GridPageView,
}
/// At most sixteen expanded paths. A descendant owns its own cache, selection
/// and worker tokens. Collapsing its parent releases the entire subtree.
pub struct GridPageBranches {
    host: WidgetId,
    schema: TableSchema,
    limits: DataLimits,
    settings: PageCacheSettings,
    capabilities: QueryCapabilities,
    base: QueryRequest,
    branches: BTreeMap<PathKey, Branch>,
    waker: Option<Wake>,
}
fn path_key(path: &[(String, CellValue)]) -> PathKey {
    path.iter()
        .map(|(field, value)| (field.clone(), projection::value_identity(value)))
        .collect()
}
impl GridPageBranches {
    pub fn new(host: WidgetId, source: &PagedDataSource) -> Self {
        Self {
            host,
            schema: source.schema().clone(),
            limits: source.limits(),
            settings: source.settings(),
            capabilities: source.capabilities(),
            base: source.query().clone(),
            branches: BTreeMap::new(),
            waker: None,
        }
    }
    pub fn set_waker(&mut self, waker: Option<Wake>) -> Result<(), ControlError> {
        for branch in self.branches.values_mut() {
            branch.page.set_waker(waker.clone())?
        }
        self.waker = waker;
        Ok(())
    }
    pub fn page(&self, path: &[(String, CellValue)]) -> Option<&GridPageView> {
        self.branches.get(&path_key(path)).map(|b| &b.page)
    }
    pub fn expand(
        &mut self,
        ui: &mut Controls,
        path: Vec<(String, CellValue)>,
    ) -> Result<Vec<(PageRequest, PageCompletionSender)>, ControlError> {
        let key = path_key(&path);
        if self.branches.contains_key(&key) {
            return Ok(vec![]);
        }
        if path.is_empty() || path.len() > 8 || self.branches.len() >= 16 {
            return Err(error("Invalid or excessive grid branch path"));
        }
        let parent = if path.len() == 1 {
            self.host
        } else {
            self.branches
                .get(&key[..key.len() - 1])
                .ok_or_else(|| error("Expand the parent path first"))?
                .host
        };
        let mut pages = PagedDataSource::new(
            self.schema.clone(),
            self.limits,
            self.settings,
            self.capabilities,
        )
        .map_err(error)?;
        pages.set_query(self.base.clone()).map_err(error)?;
        let host = ui.panel(parent, LayoutStyle::default())?;
        let create = (|| {
            ui.label(
                host,
                path.iter()
                    .map(|(f, v)| {
                        format!(
                            "{f} = {}",
                            if *v == CellValue::Null {
                                "NULL".into()
                            } else {
                                v.display()
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" / "),
                Default::default(),
            )?;
            let mut page = GridPageView::mount(
                ui,
                host,
                pages,
                LayoutStyle {
                    height: Length::Px(144.),
                    ..Default::default()
                },
            )?;
            page.set_waker(self.waker.clone())?;
            let requests = page.open_path(ui, path)?;
            let sender = page.source().completion_sender();
            Ok((
                Branch { host, page },
                requests.into_iter().map(|r| (r, sender.clone())).collect(),
            ))
        })();
        match create {
            Ok((branch, delivery)) => {
                self.branches.insert(key, branch);
                Ok(delivery)
            }
            Err(e) => {
                ui.remove(host)?;
                Err(e)
            }
        }
    }
    /// An empty path collapses every expanded group and cancels its workers.
    pub fn collapse(
        &mut self,
        ui: &mut Controls,
        path: &[(String, CellValue)],
    ) -> Result<(), ControlError> {
        let key = path_key(path);
        let targets: Vec<_> = self
            .branches
            .keys()
            .filter(|k| {
                if key.is_empty() {
                    k.len() == 1
                } else {
                    **k == key
                }
            })
            .cloned()
            .collect();
        for root in targets {
            ui.remove(self.branches[&root].host)?;
            self.branches.retain(|k, _| !k.starts_with(&root));
        }
        Ok(())
    }
    pub fn poll(
        &mut self,
        ui: &mut Controls,
    ) -> Result<Vec<(PageRequest, PageCompletionSender)>, ControlError> {
        let mut requests = Vec::new();
        for branch in self.branches.values_mut() {
            let page = &mut branch.page;
            requests.extend(
                page.take_input(ui)?
                    .into_iter()
                    .map(|r| (r, page.source().completion_sender())),
            );
            page.poll(ui)?;
        }
        Ok(requests)
    }
    pub fn navigate(
        &mut self,
        ui: &mut Controls,
        path: &[(String, CellValue)],
        offset: u64,
    ) -> Result<Vec<(PageRequest, PageCompletionSender)>, ControlError> {
        let p = &mut self
            .branches
            .get_mut(&path_key(path))
            .ok_or_else(|| error("Unknown open path"))?
            .page;
        let sender = p.source().completion_sender();
        Ok(p.navigate(ui, offset)?
            .into_iter()
            .map(|r| (r, sender.clone()))
            .collect())
    }
}
