use super::*;

const MAX_NODES: usize = 512;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug)]
pub(super) struct Reference {
    sheet: Option<String>,
    address: CellAddress,
    absolute_row: bool,
    absolute_column: bool,
}
#[derive(Clone, Debug)]
pub(super) enum Expr {
    Value(WorkbookValue),
    Reference(Reference),
    Range(Reference, Reference),
    Unary(char, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Function(String, Vec<Expr>),
}
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Word(String),
    Quoted(String),
    Symbol(char),
    Operator(String),
    Error(FormulaError),
}

fn tokenize(text: &str) -> Result<Vec<Token>, DataError> {
    if text.len() > MAX_FORMULA_BYTES {
        return Err(invalid("formula byte capacity exceeded"));
    }
    let text = text.strip_prefix('=').unwrap_or(text);
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        if tokens.len() >= MAX_NODES {
            return Err(invalid("formula token capacity exceeded"));
        }
        let c = text[cursor..].chars().next().unwrap();
        if c.is_whitespace() {
            cursor += c.len_utf8();
            continue;
        }
        if c == '"' || c == '\'' {
            cursor += 1;
            let mut value = String::new();
            let mut closed = false;
            while cursor < text.len() {
                let ch = text[cursor..].chars().next().unwrap();
                cursor += ch.len_utf8();
                if ch == c {
                    if text[cursor..].starts_with(c) {
                        value.push(c);
                        cursor += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    value.push(ch);
                }
            }
            if !closed {
                return Err(invalid("unterminated formula string"));
            }
            tokens.push(if c == '"' {
                Token::Text(value)
            } else {
                Token::Quoted(value)
            });
        } else if c == '#' {
            let errors = [
                ("#DIV/0!", FormulaError::DivisionByZero),
                ("#VALUE!", FormulaError::Value),
                ("#REF!", FormulaError::Reference),
                ("#NAME?", FormulaError::Name),
                ("#NUM!", FormulaError::Number),
                ("#CYCLE!", FormulaError::Cycle),
                ("#LIMIT!", FormulaError::Limit),
            ];
            let Some((name, error)) = errors
                .iter()
                .find(|(name, _)| text[cursor..].starts_with(name))
            else {
                return Err(invalid("unknown formula error literal"));
            };
            cursor += name.len();
            tokens.push(Token::Error(*error));
        } else if c.is_ascii_digit() || c == '.' {
            let start = cursor;
            cursor += 1;
            while cursor < text.len()
                && (text.as_bytes()[cursor].is_ascii_digit() || text.as_bytes()[cursor] == b'.')
            {
                cursor += 1;
            }
            if matches!(text.as_bytes().get(cursor), Some(b'e' | b'E')) {
                cursor += 1;
                if matches!(text.as_bytes().get(cursor), Some(b'+' | b'-')) {
                    cursor += 1;
                }
                while cursor < text.len() && text.as_bytes()[cursor].is_ascii_digit() {
                    cursor += 1;
                }
            }
            let value: f64 = text[start..cursor]
                .parse()
                .map_err(|_| invalid("invalid formula number"))?;
            if !value.is_finite() {
                return Err(invalid("nonfinite formula number"));
            }
            tokens.push(Token::Number(value));
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let start = cursor;
            cursor += c.len_utf8();
            while cursor < text.len() {
                let ch = text[cursor..].chars().next().unwrap();
                if !ch.is_alphanumeric() && !matches!(ch, '_' | '$' | '.') {
                    break;
                }
                cursor += ch.len_utf8();
            }
            tokens.push(Token::Word(text[start..cursor].into()));
        } else if "+-*/^&=<>".contains(c) {
            cursor += 1;
            let mut op = c.to_string();
            if matches!(c, '<' | '>')
                && (text[cursor..].starts_with('=')
                    || (c == '<' && text[cursor..].starts_with('>')))
            {
                op.push(text.as_bytes()[cursor] as char);
                cursor += 1;
            }
            tokens.push(Token::Operator(op));
        } else if "(),:!%".contains(c) {
            cursor += 1;
            tokens.push(Token::Symbol(c));
        } else {
            return Err(invalid(format!("unsupported formula character: {c}")));
        }
    }
    Ok(tokens)
}
struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}
pub(super) fn parse(text: &str) -> Result<Expr, DataError> {
    let mut parser = Parser {
        tokens: tokenize(text)?,
        cursor: 0,
    };
    let expr = parser.expression(0, 0)?;
    if parser.cursor != parser.tokens.len() {
        return Err(invalid("unexpected formula tokens"));
    }
    let mut pending = vec![(&expr, 0)];
    while let Some((node, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err(invalid("formula AST depth limit exceeded"));
        }
        match node {
            Expr::Unary(_, value) => pending.push((value, depth + 1)),
            Expr::Binary(_, left, right) => {
                pending.push((left, depth + 1));
                pending.push((right, depth + 1));
            }
            Expr::Function(_, args) => pending.extend(args.iter().map(|arg| (arg, depth + 1))),
            _ => {}
        }
    }
    Ok(expr)
}
impl Parser {
    fn symbol(&mut self, c: char) -> bool {
        if self.tokens.get(self.cursor) == Some(&Token::Symbol(c)) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn expression(&mut self, minimum: u8, depth: usize) -> Result<Expr, DataError> {
        if depth > MAX_DEPTH * 3 {
            return Err(invalid("formula nesting limit exceeded"));
        }
        let token = self
            .tokens
            .get(self.cursor)
            .cloned()
            .ok_or_else(|| invalid("missing formula operand"))?;
        self.cursor += 1;
        let mut left = match token {
            Token::Number(n) => Expr::Value(WorkbookValue::Scalar(CellValue::Real(n))),
            Token::Text(s) => Expr::Value(WorkbookValue::Scalar(CellValue::Text(s))),
            Token::Error(e) => Expr::Value(WorkbookValue::Error(e)),
            Token::Operator(op) if op == "+" || op == "-" => Expr::Unary(
                op.chars().next().unwrap(),
                Box::new(self.expression(6, depth + 1)?),
            ),
            Token::Symbol('(') => {
                let expr = self.expression(0, depth + 1)?;
                if !self.symbol(')') {
                    return Err(invalid("missing formula closing parenthesis"));
                }
                expr
            }
            Token::Word(word) | Token::Quoted(word) => {
                if self.symbol('(') {
                    let mut args = Vec::new();
                    if !self.symbol(')') {
                        loop {
                            args.push(self.expression(0, depth + 1)?);
                            if self.symbol(')') {
                                break;
                            }
                            if !self.symbol(',') {
                                return Err(invalid("expected function argument separator"));
                            }
                        }
                    }
                    Expr::Function(word.to_ascii_uppercase(), args)
                } else if self.symbol('!') {
                    let Some(Token::Word(address)) = self.tokens.get(self.cursor).cloned() else {
                        return Err(invalid("missing cell after sheet name"));
                    };
                    self.cursor += 1;
                    self.reference_range(Some(word), &address)?
                } else if word.eq_ignore_ascii_case("TRUE") || word.eq_ignore_ascii_case("FALSE") {
                    Expr::Value(WorkbookValue::Scalar(CellValue::Bool(
                        word.eq_ignore_ascii_case("TRUE"),
                    )))
                } else {
                    self.reference_range(None, &word)?
                }
            }
            _ => return Err(invalid("unexpected formula operand")),
        };
        loop {
            if self.tokens.get(self.cursor) == Some(&Token::Symbol('%')) {
                self.cursor += 1;
                left = Expr::Binary(
                    "/".into(),
                    Box::new(left),
                    Box::new(Expr::Value(WorkbookValue::Scalar(CellValue::Real(100.0)))),
                );
                continue;
            }
            let Some(Token::Operator(op)) = self.tokens.get(self.cursor).cloned() else {
                break;
            };
            let priority = match op.as_str() {
                "=" | "<>" | "<" | ">" | "<=" | ">=" => 1,
                "&" => 2,
                "+" | "-" => 3,
                "*" | "/" => 4,
                "^" => 5,
                _ => break,
            };
            if priority < minimum {
                break;
            }
            self.cursor += 1;
            let right = self.expression(priority + u8::from(op != "^"), depth + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn reference_range(&mut self, sheet: Option<String>, text: &str) -> Result<Expr, DataError> {
        let first = reference(sheet.clone(), text)?;
        if self.symbol(':') {
            let Some(Token::Word(last)) = self.tokens.get(self.cursor).cloned() else {
                return Err(invalid("missing range end"));
            };
            self.cursor += 1;
            let second = reference(sheet, &last)?;
            CellRange::new(first.address, second.address)?;
            Ok(Expr::Range(first, second))
        } else {
            Ok(Expr::Reference(first))
        }
    }
}
fn reference(sheet: Option<String>, text: &str) -> Result<Reference, DataError> {
    let absolute_column = text.starts_with('$');
    let text = text.strip_prefix('$').unwrap_or(text);
    let split = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    let (col, row) = text.split_at(split);
    let absolute_row = row.starts_with('$');
    let row = row.strip_prefix('$').unwrap_or(row);
    let address = CellAddress::parse(&format!("{col}{row}"))?;
    Ok(Reference {
        sheet,
        address,
        absolute_column,
        absolute_row,
    })
}
fn id(
    book: &Workbook,
    context: usize,
    reference: &Reference,
) -> Result<WorkbookCellId, FormulaError> {
    let sheet = if let Some(name) = &reference.sheet {
        book.sheets
            .iter()
            .position(|s| s.name.to_lowercase() == name.to_lowercase())
            .ok_or(FormulaError::Reference)?
    } else {
        context
    };
    Ok(WorkbookCellId {
        sheet,
        address: reference.address,
    })
}
fn collect(book: &Workbook, context: usize, expr: &Expr, refs: &mut BTreeSet<WorkbookCellId>) {
    match expr {
        Expr::Reference(r) => {
            if let Ok(id) = id(book, context, r) {
                refs.insert(id);
            }
        }
        Expr::Range(a, b) => {
            if let Ok(id) = id(book, context, a) {
                for address in (CellRange {
                    start: a.address,
                    end: b.address,
                })
                .addresses()
                {
                    refs.insert(WorkbookCellId {
                        sheet: id.sheet,
                        address,
                    });
                }
            }
        }
        Expr::Unary(_, e) => collect(book, context, e, refs),
        Expr::Binary(_, a, b) => {
            collect(book, context, a, refs);
            collect(book, context, b, refs);
        }
        Expr::Function(_, args) => {
            for arg in args {
                collect(book, context, arg, refs);
            }
        }
        _ => {}
    }
}
pub(super) fn recalculate(book: &mut Workbook) -> Result<(), DataError> {
    let mut expressions = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut total_refs = 0;
    for (sheet, s) in book.sheets.iter().enumerate() {
        for (address, cell) in &s.cells {
            if let CellInput::Formula(text) = &cell.input {
                let expr = parse(text)?;
                let key = WorkbookCellId {
                    sheet,
                    address: *address,
                };
                let mut refs = BTreeSet::new();
                collect(book, sheet, &expr, &mut refs);
                total_refs += refs.len();
                if total_refs > 262_144 {
                    return Err(invalid("workbook dependency capacity exceeded"));
                }
                dependencies.insert(key.clone(), refs);
                expressions.insert(key, expr);
            }
        }
    }
    let mut evaluator = Evaluator {
        book,
        expressions: &expressions,
        values: BTreeMap::new(),
        active: BTreeSet::new(),
        budget: 2_000_000,
        depth: 0,
    };
    for (sheet, s) in book.sheets.iter().enumerate() {
        for address in s.cells.keys() {
            let key = WorkbookCellId {
                sheet,
                address: *address,
            };
            let value = evaluator.cell(&key);
            // A limit reached before evaluation must still publish an error,
            // otherwise an unvisited formula would appear to be an empty cell.
            evaluator.values.entry(key).or_insert(value);
        }
    }
    book.values = evaluator.values;
    book.dependencies = dependencies;
    Ok(())
}
struct Evaluator<'a> {
    book: &'a Workbook,
    expressions: &'a BTreeMap<WorkbookCellId, Expr>,
    values: BTreeMap<WorkbookCellId, WorkbookValue>,
    active: BTreeSet<WorkbookCellId>,
    budget: usize,
    depth: usize,
}
type EvalResult = Result<CellValue, FormulaError>;
fn as_result(value: WorkbookValue) -> EvalResult {
    match value {
        WorkbookValue::Scalar(v) => Ok(v),
        WorkbookValue::Error(e) => Err(e),
    }
}
fn numeric(value: &CellValue) -> Result<f64, FormulaError> {
    match value {
        CellValue::Null => Ok(0.0),
        CellValue::Bool(b) => Ok(f64::from(u8::from(*b))),
        CellValue::Integer(n) => Ok(*n as f64),
        CellValue::Real(n) => Ok(*n),
        CellValue::Decimal(n) => n.to_string().parse().map_err(|_| FormulaError::Number),
        _ => Err(FormulaError::Value),
    }
}
fn finite(value: f64) -> EvalResult {
    if value.is_finite() {
        Ok(CellValue::Real(value))
    } else {
        Err(FormulaError::Number)
    }
}
impl Evaluator<'_> {
    fn cell(&mut self, key: &WorkbookCellId) -> WorkbookValue {
        if let Some(value) = self.values.get(key) {
            return value.clone();
        }
        // Literal cells do not need formula evaluation, even when an earlier
        // formula has exhausted this recalculation's budget.
        let literal = match self.book.sheets[key.sheet].cells.get(&key.address) {
            None => Some(WorkbookValue::Scalar(CellValue::Null)),
            Some(WorkbookCell {
                input: CellInput::Value(v),
                ..
            }) => Some(WorkbookValue::Scalar(v.clone())),
            Some(WorkbookCell {
                input: CellInput::Error(e),
                ..
            }) => Some(WorkbookValue::Error(*e)),
            _ => None,
        };
        if let Some(value) = literal {
            self.values.insert(key.clone(), value.clone());
            return value;
        }
        if self.active.contains(key) {
            return WorkbookValue::Error(FormulaError::Cycle);
        }
        if self.active.len() >= 128 || self.budget == 0 {
            return WorkbookValue::Error(FormulaError::Limit);
        }
        self.budget -= 1;
        self.active.insert(key.clone());
        let value = match self.book.sheets[key.sheet].cells.get(&key.address) {
            None => Ok(CellValue::Null),
            Some(WorkbookCell {
                input: CellInput::Value(v),
                ..
            }) => Ok(v.clone()),
            Some(WorkbookCell {
                input: CellInput::Error(e),
                ..
            }) => Err(*e),
            _ => {
                let expr = &self.expressions[key];
                self.eval(key.sheet, expr)
            }
        };
        self.active.remove(key);
        let value = match value {
            Ok(v) => WorkbookValue::Scalar(v),
            Err(e) => WorkbookValue::Error(e),
        };
        self.values.insert(key.clone(), value.clone());
        value
    }
    fn eval(&mut self, context: usize, expr: &Expr) -> EvalResult {
        if self.depth >= 64 {
            return Err(FormulaError::Limit);
        }
        self.depth += 1;
        let result = self.eval_inner(context, expr);
        self.depth -= 1;
        result
    }
    fn eval_inner(&mut self, context: usize, expr: &Expr) -> EvalResult {
        if self.budget == 0 {
            return Err(FormulaError::Limit);
        }
        self.budget -= 1;
        match expr {
            Expr::Value(v) => as_result(v.clone()),
            Expr::Reference(r) => {
                let key = id(self.book, context, r)?;
                as_result(self.cell(&key))
            }
            Expr::Range(..) => Err(FormulaError::Value),
            Expr::Unary(op, value) => {
                let n = numeric(&self.eval(context, value)?)?;
                finite(if *op == '-' { -n } else { n })
            }
            Expr::Binary(op, left, right) => {
                let left = self.eval(context, left)?;
                let right = self.eval(context, right)?;
                if op == "&" {
                    let text = format!("{}{}", left.display(), right.display());
                    return if text.len() <= MAX_TEXT_BYTES {
                        Ok(CellValue::Text(text))
                    } else {
                        Err(FormulaError::Limit)
                    };
                }
                if matches!(op.as_str(), "=" | "<>" | "<" | ">" | "<=" | ">=") {
                    let ordering = match (&left, &right) {
                        (CellValue::Text(a), CellValue::Text(b)) => {
                            a.to_lowercase().cmp(&b.to_lowercase())
                        }
                        _ => numeric(&left)?
                            .partial_cmp(&numeric(&right)?)
                            .ok_or(FormulaError::Number)?,
                    };
                    return Ok(CellValue::Bool(match op.as_str() {
                        "=" => ordering.is_eq(),
                        "<>" => !ordering.is_eq(),
                        "<" => ordering.is_lt(),
                        ">" => ordering.is_gt(),
                        "<=" => !ordering.is_gt(),
                        _ => !ordering.is_lt(),
                    }));
                }
                let a = numeric(&left)?;
                let b = numeric(&right)?;
                finite(match op.as_str() {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => {
                        if b == 0.0 {
                            return Err(FormulaError::DivisionByZero);
                        }
                        a / b
                    }
                    "^" => a.powf(b),
                    _ => return Err(FormulaError::Name),
                })
            }
            Expr::Function(name, args) => {
                if name == "IF" {
                    if args.len() != 3 {
                        return Err(FormulaError::Value);
                    }
                    let condition = numeric(&self.eval(context, &args[0])?)? != 0.0;
                    return self.eval(context, &args[if condition { 1 } else { 2 }]);
                }
                if !matches!(
                    name.as_str(),
                    "SUM" | "AVERAGE" | "MIN" | "MAX" | "COUNT" | "ABS" | "ROUND"
                ) {
                    return Err(FormulaError::Name);
                }
                let mut values = Vec::new();
                for arg in args {
                    if let Expr::Range(a, b) = arg {
                        let sheet = id(self.book, context, a)?.sheet;
                        for address in (CellRange {
                            start: a.address,
                            end: b.address,
                        })
                        .addresses()
                        {
                            // Cached values still cost a range visit. Otherwise
                            // repeated text ranges bypass the evaluation limit.
                            if self.budget == 0 {
                                return Err(FormulaError::Limit);
                            }
                            self.budget -= 1;
                            let value = as_result(self.cell(&WorkbookCellId { sheet, address }))?;
                            if matches!(
                                value,
                                CellValue::Null | CellValue::Text(_) | CellValue::Bool(_)
                            ) {
                                continue;
                            }
                            values.push(numeric(&value)?);
                        }
                    } else {
                        let value = self.eval(context, arg)?;
                        if name == "COUNT"
                            && matches!(
                                value,
                                CellValue::Null | CellValue::Text(_) | CellValue::Bool(_)
                            )
                        {
                            continue;
                        }
                        values.push(numeric(&value)?);
                    }
                    if values.len() > MAX_CELLS {
                        return Err(FormulaError::Limit);
                    }
                }
                finite(match name.as_str() {
                    "SUM" => values.iter().sum(),
                    "COUNT" => values.len() as f64,
                    "AVERAGE" => {
                        if values.is_empty() {
                            return Err(FormulaError::DivisionByZero);
                        }
                        values.iter().sum::<f64>() / values.len() as f64
                    }
                    "MIN" => values.into_iter().reduce(f64::min).unwrap_or(0.0),
                    "MAX" => values.into_iter().reduce(f64::max).unwrap_or(0.0),
                    "ABS" if values.len() == 1 => values[0].abs(),
                    "ROUND"
                        if values.len() == 2
                            && values[1].fract() == 0.0
                            && values[1].abs() <= 15.0 =>
                    {
                        let factor = 10f64.powi(values[1] as i32);
                        (values[0] * factor).round() / factor
                    }
                    _ => return Err(FormulaError::Value),
                })
            }
        }
    }
}

pub(super) fn translate(text: &str, rows: i64, cols: i64) -> Result<String, DataError> {
    let mut expr = parse(text)?;
    shift(&mut expr, rows, cols);
    Ok(format!("={}", render(&expr)))
}
fn shift(expr: &mut Expr, rows: i64, cols: i64) {
    fn move_ref(r: &mut Reference, rows: i64, cols: i64) -> bool {
        let row = i64::from(r.address.row) + if r.absolute_row { 0 } else { rows };
        let column = i64::from(r.address.column) + if r.absolute_column { 0 } else { cols };
        if !(0..i64::from(MAX_ROWS)).contains(&row)
            || !(0..i64::from(MAX_COLUMNS)).contains(&column)
        {
            return false;
        }
        r.address = CellAddress {
            row: row as u32,
            column: column as u32,
        };
        true
    }
    match expr {
        Expr::Reference(r) => {
            if !move_ref(r, rows, cols) {
                *expr = Expr::Value(WorkbookValue::Error(FormulaError::Reference));
            }
        }
        Expr::Range(a, b) => {
            if !move_ref(a, rows, cols) || !move_ref(b, rows, cols) {
                *expr = Expr::Value(WorkbookValue::Error(FormulaError::Reference));
            }
        }
        Expr::Unary(_, e) => shift(e, rows, cols),
        Expr::Binary(_, a, b) => {
            shift(a, rows, cols);
            shift(b, rows, cols);
        }
        Expr::Function(_, args) => {
            for a in args {
                shift(a, rows, cols);
            }
        }
        _ => {}
    }
}
fn render_ref(r: &Reference, include_sheet: bool) -> String {
    let a1 = r.address.a1();
    let split = a1.bytes().take_while(u8::is_ascii_alphabetic).count();
    format!(
        "{}{}{}{}{}",
        if include_sheet {
            r.sheet
                .as_ref()
                .map(|s| format!("'{}'!", s.replace('\'', "''")))
                .unwrap_or_default()
        } else {
            String::new()
        },
        if r.absolute_column { "$" } else { "" },
        &a1[..split],
        if r.absolute_row { "$" } else { "" },
        &a1[split..]
    )
}
fn render(expr: &Expr) -> String {
    match expr {
        Expr::Value(WorkbookValue::Error(e)) => e.display().into(),
        Expr::Value(WorkbookValue::Scalar(CellValue::Text(t))) => {
            format!("\"{}\"", t.replace('"', "\"\""))
        }
        Expr::Value(WorkbookValue::Scalar(CellValue::Bool(v))) => {
            if *v { "TRUE" } else { "FALSE" }.into()
        }
        Expr::Value(v) => v.display(),
        Expr::Reference(r) => render_ref(r, true),
        Expr::Range(a, b) => format!("{}:{}", render_ref(a, true), render_ref(b, false)),
        Expr::Unary(op, e) => format!("({op}{})", render(e)),
        Expr::Binary(op, a, b) => format!("({}{op}{})", render(a), render(b)),
        Expr::Function(name, args) => format!(
            "{name}({})",
            args.iter().map(render).collect::<Vec<_>>().join(",")
        ),
    }
}
