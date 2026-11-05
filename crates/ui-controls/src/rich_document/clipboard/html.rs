use super::super::{DocumentError, RichRun, RunStyle};
use super::{MAX_HTML_BYTES, RichClipboardFragment, css};
use html5ever::{
    buffer_queue::BufferQueue,
    tokenizer::{Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};
use std::cell::RefCell;

#[derive(Clone)]
struct Frame {
    tag: String,
    style: RunStyle,
    preserve: bool,
}
struct Builder {
    stack: Vec<Frame>,
    runs: Vec<RichRun>,
    bytes: usize,
    tokens: usize,
    pending_space: Option<RunStyle>,
    pending_break: bool,
    error: Option<DocumentError>,
}
impl Builder {
    fn raw(&mut self, text: &str, style: &RunStyle) -> Result<(), DocumentError> {
        if text.is_empty() {
            return Ok(());
        }
        if self.bytes + text.len() > 1024 * 1024 {
            return Err(DocumentError::Limit("clipboard text"));
        }
        self.bytes += text.len();
        if let Some(last) = self.runs.last_mut()
            && last.style == *style
        {
            last.text.push_str(text);
        } else {
            if self.runs.len() == 256 {
                return Err(DocumentError::Limit("256 clipboard runs"));
            }
            self.runs.push(RichRun {
                text: text.into(),
                style: style.clone(),
            });
        }
        Ok(())
    }
    fn flush(&mut self, style: &RunStyle) -> Result<(), DocumentError> {
        if self.pending_break {
            self.pending_break = false;
            self.pending_space = None;
            if !self.runs.is_empty() && !self.runs.last().unwrap().text.ends_with('\n') {
                self.raw("\n", style)?;
            }
        } else if let Some(space) = self.pending_space.take()
            && self
                .runs
                .last()
                .is_some_and(|r| !r.text.ends_with([' ', '\n', '\t']))
        {
            self.raw(" ", &space)?;
        }
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), DocumentError> {
        let frame = self.stack.last().unwrap().clone();
        if frame.preserve {
            self.flush(&frame.style)?;
            return self.raw(text, &frame.style);
        }
        for c in text.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c') {
                self.pending_space
                    .get_or_insert_with(|| frame.style.clone());
            } else {
                self.flush(&frame.style)?;
                self.raw(c.encode_utf8(&mut [0; 4]), &frame.style)?;
            }
        }
        Ok(())
    }
    fn tag(&mut self, tag: Tag) -> Result<(), DocumentError> {
        let name = tag.name.as_ref();
        let block = matches!(name, "p" | "div" | "pre");
        if !matches!(
            name,
            "b" | "strong" | "i" | "em" | "u" | "span" | "br" | "p" | "div" | "pre"
        ) {
            return Err(DocumentError::Unsupported("clipboard HTML element".into()));
        }
        if tag.kind == TagKind::EndTag {
            if tag.self_closing
                || !tag.attrs.is_empty()
                || self.stack.len() == 1
                || self.stack.last().unwrap().tag != name
            {
                return Err(DocumentError::Invalid("clipboard HTML nesting".into()));
            }
            self.stack.pop();
            if block {
                self.pending_break = true;
                self.pending_space = None;
            }
            return Ok(());
        }
        if tag.attrs.len() > 32 {
            return Err(DocumentError::Limit("clipboard attributes"));
        }
        if tag.had_duplicate_attributes || (tag.self_closing && name != "br") {
            return Err(DocumentError::Invalid(
                "clipboard HTML attributes or self-closing element".into(),
            ));
        }
        let mut frame = self.stack.last().unwrap().clone();
        frame.tag = name.into();
        match name {
            "b" | "strong" => frame.style.bold = true,
            "i" | "em" => frame.style.italic = true,
            "u" => frame.style.underline = true,
            "pre" => frame.preserve = true,
            _ => (),
        }
        for attr in &tag.attrs {
            if &*attr.name.local != "style" || !attr.name.ns.is_empty() {
                return Err(DocumentError::Unsupported(
                    "clipboard HTML attribute".into(),
                ));
            }
            css::apply(&attr.value, &mut frame.style, &mut frame.preserve)?;
        }
        if name == "br" {
            self.flush(&frame.style)?;
            self.raw("\n", &frame.style)?;
        } else {
            if self.stack.len() >= 65 {
                return Err(DocumentError::Limit("64 clipboard element levels"));
            }
            if block {
                self.pending_break = true;
                self.pending_space = None;
            }
            self.stack.push(frame);
        }
        Ok(())
    }
    fn token(&mut self, token: Token) -> Result<(), DocumentError> {
        self.tokens += 1;
        if self.tokens > 16384 {
            return Err(DocumentError::Limit("16384 clipboard tokens"));
        }
        match token {
            Token::CharacterTokens(text) => self.text(&text),
            Token::TagToken(tag) => self.tag(tag),
            Token::CommentToken(_) => Ok(()),
            Token::EOFToken if self.stack.len() == 1 => Ok(()),
            _ => Err(DocumentError::Invalid(
                "clipboard HTML syntax or unclosed element".into(),
            )),
        }
    }
}
struct Sink(RefCell<Builder>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut builder = self.0.borrow_mut();
        if builder.error.is_none()
            && let Err(error) = builder.token(token)
        {
            builder.error = Some(error);
        }
        TokenSinkResult::Continue
    }
}
pub(super) fn parse(source: &str, style: RunStyle) -> Result<RichClipboardFragment, DocumentError> {
    style.validate()?;
    if source.len() > MAX_HTML_BYTES {
        return Err(DocumentError::Limit("4 MiB clipboard HTML"));
    }
    if source.contains('\0') {
        return Err(DocumentError::Invalid("clipboard NUL".into()));
    }
    let tokenizer = Tokenizer::new(
        Sink(RefCell::new(Builder {
            stack: vec![Frame {
                tag: String::new(),
                style,
                preserve: false,
            }],
            runs: Vec::new(),
            bytes: 0,
            tokens: 0,
            pending_space: None,
            pending_break: false,
            error: None,
        })),
        TokenizerOpts {
            discard_bom: false,
            ..Default::default()
        },
    );
    let input = BufferQueue::default();
    input.push_back(source.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let builder = tokenizer.sink.0.into_inner();
    if let Some(error) = builder.error {
        return Err(error);
    }
    RichClipboardFragment::new(builder.runs)
}
