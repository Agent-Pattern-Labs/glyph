use serde::Serialize;
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgramAst {
    pub goal: Option<String>,
    pub context: Option<ContextAst>,
    pub flows: Vec<FlowAst>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextAst {
    pub entries: Vec<ObjectEntryAst>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowAst {
    pub name: String,
    pub steps: Vec<StatementAst>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum StatementAst {
    ToolCall(ToolCallAst),
    RepairBlock(RepairBlockAst),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCallAst {
    pub op: String,
    pub args: Vec<CallArgAst>,
    #[serde(rename = "assignTo", skip_serializing_if = "Option::is_none")]
    pub assign_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepairBlockAst {
    pub target: String,
    pub report: String,
    pub max: usize,
    pub steps: Vec<StatementAst>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CallArgAst {
    pub name: Option<String>,
    pub value: ExpressionAst,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ExpressionAst {
    String(String),
    Number(Number),
    Boolean(bool),
    Array(Vec<ExpressionAst>),
    Object(Vec<ObjectEntryAst>),
    VarRef(String),
    CtxRef(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectEntryAst {
    pub key: String,
    pub value: ExpressionAst,
}
