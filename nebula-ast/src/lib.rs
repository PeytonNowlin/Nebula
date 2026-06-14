pub type Span = std::ops::Range<usize>;

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Spanned<TopLevel>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevel {
    Import(Spanned<String>),
    Sector(Spanned<Sector>),
    Mission(Spanned<Mission>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sector {
    pub name: Spanned<String>,
    pub items: Vec<SectorItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SectorItem {
    Fn(Spanned<FnDecl>),
    Struct(Spanned<StructDecl>),
    Probe(Spanned<ProbeDecl>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mission {
    pub name: Spanned<String>,
    pub items: Vec<MissionItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MissionItem {
    Stmt(Spanned<Stmt>),
    Probe(Spanned<ProbeDecl>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: Spanned<String>,
    pub params: Vec<Spanned<Param>>,
    pub return_type: Spanned<Type>,
    pub body: Vec<Spanned<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: Spanned<String>,
    pub fields: Vec<Spanned<FieldDecl>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: Spanned<String>,
    pub ty: Spanned<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeDecl {
    pub name: Spanned<String>,
    pub params: Vec<Spanned<Param>>,
    pub return_type: Spanned<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Spanned<String>,
    pub ty: Spanned<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        mutable: bool,
        name: Spanned<String>,
        ty: Spanned<Type>,
        value: Spanned<Expr>,
    },
    Set {
        name: Spanned<String>,
        value: Spanned<Expr>,
    },
    If {
        condition: Spanned<Expr>,
        then_block: Vec<Spanned<Stmt>>,
        else_block: Option<Vec<Spanned<Stmt>>>,
    },
    While {
        condition: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
    },
    Emit(Spanned<Expr>),
    Return(Spanned<Expr>),
    Expr(Spanned<Expr>),
    Call {
        name: Spanned<String>,
        args: Vec<Spanned<NamedArg>>,
    },
    Telemetry {
        body: Vec<Spanned<Stmt>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    pub name: Spanned<String>,
    pub value: Spanned<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    Some(Box<Spanned<Expr>>),
    Ident(Spanned<String>),
    FieldAccess {
        object: Box<Spanned<Expr>>,
        field: Spanned<String>,
    },
    Call {
        callee: Spanned<String>,
        args: Vec<Spanned<Expr>>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Spanned<Expr>>,
    },
    Binary {
        left: Box<Spanned<Expr>>,
        op: BinaryOp,
        right: Box<Spanned<Expr>>,
    },
    List(Vec<Spanned<Expr>>),
    Map(Vec<Spanned<MapEntry>>),
    StructLit {
        name: Spanned<String>,
        fields: Vec<Spanned<FieldInit>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Plus,
    Minus,
    Times,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    pub key: Spanned<Expr>,
    pub value: Spanned<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: Spanned<String>,
    pub value: Spanned<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Void,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Option(Box<Type>),
    /// Type-checker sentinel: bare `None` unifies with any `Option<T>`.
    NoneValue,
    Fn(Vec<Type>, Box<Type>),
    Named(String),
}

impl Type {
    pub fn display(&self) -> String {
        match self {
            Type::Int => "Int".into(),
            Type::Float => "Float".into(),
            Type::Bool => "Bool".into(),
            Type::Str => "Str".into(),
            Type::Void => "Void".into(),
            Type::List(inner) => format!("List<{}>", inner.display()),
            Type::Map(k, v) => format!("Map<{}, {}>", k.display(), v.display()),
            Type::Option(inner) => format!("Option<{}>", inner.display()),
            Type::NoneValue => "None".into(),
            Type::Fn(params, ret) => {
                let ps: Vec<_> = params.iter().map(Type::display).collect();
                format!("fn({}) -> {}", ps.join(", "), ret.display())
            }
            Type::Named(name) => name.clone(),
        }
    }
}

pub trait SpanExt {
    fn span(&self) -> Span;
}

impl<T: SpanExt> SpanExt for Spanned<T> {
    fn span(&self) -> Span {
        self.span.clone()
    }
}

impl SpanExt for Expr {
    fn span(&self) -> Span {
        0..0
    }
}