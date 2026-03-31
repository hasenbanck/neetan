use std::collections::HashMap;

pub(crate) struct Module {
    pub(crate) entry_points: Vec<EntryPoint>,
    pub(crate) types: HashMap<u32, Type>,
    pub(crate) constants: HashMap<u32, Value>,
    pub(crate) global_variables: HashMap<u32, GlobalVariable>,
    pub(crate) functions: Vec<Function>,
    pub(crate) decorations: HashMap<u32, Vec<Decoration>>,
    pub(crate) member_decorations: HashMap<(u32, u32), Vec<Decoration>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionModel {
    Vertex,
    Fragment,
}

pub(crate) struct EntryPoint {
    pub(crate) execution_model: ExecutionModel,
    pub(crate) function_id: u32,
    pub(crate) name: String,
    pub(crate) interface_ids: Vec<u32>,
}

pub(crate) struct Function {
    pub(crate) result_id: u32,
    pub(crate) blocks: Vec<Block>,
}

pub(crate) struct Block {
    pub(crate) label_id: u32,
    pub(crate) phi_instructions: Vec<Phi>,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) terminator: Terminator,
}

pub(crate) struct Phi {
    pub(crate) result_id: u32,
    pub(crate) incoming: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageClass {
    Input,
    Output,
    StorageBuffer,
    Function,
}

pub(crate) struct GlobalVariable {
    pub(crate) type_id: u32,
    pub(crate) storage_class: StorageClass,
}

#[derive(Debug, Clone)]
pub(crate) enum Decoration {
    Binding(u32),
    BuiltIn(BuiltIn),
    Location(u32),
    Offset(u32),
    ArrayStride(u32),
    Block,
    NonWritable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltIn {
    Position,
    VertexIndex,
    FragCoord,
}

#[derive(Debug, Clone)]
pub(crate) enum Type {
    Void,
    Bool,
    Int { width: u32 },
    Float { width: u32 },
    Vector { component_type_id: u32, count: u32 },
    Array { element_type_id: u32 },
    RuntimeArray { element_type_id: u32 },
    Struct { member_type_ids: Vec<u32> },
    Pointer { pointee_type_id: u32 },
    Function,
}

#[derive(Debug, Clone)]
pub(crate) enum Value {
    Bool(bool),
    U32(u32),
    F32(f32),
    Composite(Vec<u32>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlslExtInst {
    Pow,
    UMin,
    UMax,
    SMax,
}

#[derive(Debug, Clone)]
pub(crate) enum Instruction {
    Load {
        result_id: u32,
        pointer_id: u32,
    },
    Store {
        pointer_id: u32,
        value_id: u32,
    },
    AccessChain {
        result_id: u32,
        result_type_id: u32,
        base_id: u32,
        indexes: Vec<u32>,
    },
    VectorShuffle {
        result_id: u32,
        vector1_id: u32,
        vector2_id: u32,
        components: Vec<u32>,
    },
    CompositeConstruct {
        result_id: u32,
        constituent_ids: Vec<u32>,
    },
    CompositeExtract {
        result_id: u32,
        composite_id: u32,
        indexes: Vec<u32>,
    },
    ConvertFToS {
        result_id: u32,
        operand_id: u32,
    },
    ConvertUToF {
        result_id: u32,
        operand_id: u32,
    },
    Bitcast {
        result_id: u32,
        result_type_id: u32,
        operand_id: u32,
    },
    IAdd {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    FAdd {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    ISub {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    IMul {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    FMul {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    UDiv {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    UMod {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    BitwiseAnd {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    BitwiseOr {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    ShiftRightLogical {
        result_id: u32,
        base_id: u32,
        shift_id: u32,
    },
    ShiftLeftLogical {
        result_id: u32,
        base_id: u32,
        shift_id: u32,
    },
    IEqual {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    INotEqual {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    UGreaterThan {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    UGreaterThanEqual {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    SGreaterThanEqual {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    ULessThan {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    SLessThan {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    SLessThanEqual {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    FOrdLessThanEqual {
        result_id: u32,
        left_id: u32,
        right_id: u32,
    },
    LogicalNot {
        result_id: u32,
        operand_id: u32,
    },
    Select {
        result_id: u32,
        condition_id: u32,
        true_id: u32,
        false_id: u32,
    },
    ExtInst {
        result_id: u32,
        instruction: GlslExtInst,
        operand_ids: Vec<u32>,
    },
    SelectionMerge,
    LoopMerge,
    Variable {
        result_id: u32,
    },
}

pub(crate) enum Terminator {
    Branch {
        target_label: u32,
    },
    BranchConditional {
        condition_id: u32,
        true_label: u32,
        false_label: u32,
    },
    Switch {
        selector_id: u32,
        default_label: u32,
        targets: Vec<(u32, u32)>,
    },
    Return,
}
