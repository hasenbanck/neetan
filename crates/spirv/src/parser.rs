use std::collections::HashMap;

use crate::{
    error::Error,
    module::{
        Block, BuiltIn, Decoration, EntryPoint, ExecutionModel, Function, GlobalVariable,
        GlslExtInst, Instruction, Module, Phi, StorageClass, Terminator, Type,
        Value as ConstantValue,
    },
};

const SPIRV_MAGIC: u32 = 0x07230203;

const OP_SOURCE: u16 = 3;
const OP_NAME: u16 = 5;
const OP_MEMBER_NAME: u16 = 6;
const OP_EXTENSION: u16 = 10;
const OP_EXT_INST_IMPORT: u16 = 11;
const OP_EXT_INST: u16 = 12;
const OP_MEMORY_MODEL: u16 = 14;
const OP_ENTRY_POINT: u16 = 15;
const OP_EXECUTION_MODE: u16 = 16;
const OP_CAPABILITY: u16 = 17;
const OP_TYPE_VOID: u16 = 19;
const OP_TYPE_BOOL: u16 = 20;
const OP_TYPE_INT: u16 = 21;
const OP_TYPE_FLOAT: u16 = 22;
const OP_TYPE_VECTOR: u16 = 23;
const OP_TYPE_ARRAY: u16 = 28;
const OP_TYPE_RUNTIME_ARRAY: u16 = 29;
const OP_TYPE_STRUCT: u16 = 30;
const OP_TYPE_POINTER: u16 = 32;
const OP_TYPE_FUNCTION: u16 = 33;
const OP_CONSTANT_TRUE: u16 = 41;
const OP_CONSTANT_FALSE: u16 = 42;
const OP_CONSTANT: u16 = 43;
const OP_CONSTANT_COMPOSITE: u16 = 44;
const OP_FUNCTION: u16 = 54;
const OP_FUNCTION_END: u16 = 56;
const OP_VARIABLE: u16 = 59;
const OP_LOAD: u16 = 61;
const OP_STORE: u16 = 62;
const OP_ACCESS_CHAIN: u16 = 65;
const OP_DECORATE: u16 = 71;
const OP_MEMBER_DECORATE: u16 = 72;
const OP_VECTOR_SHUFFLE: u16 = 79;
const OP_COMPOSITE_CONSTRUCT: u16 = 80;
const OP_COMPOSITE_EXTRACT: u16 = 81;
const OP_CONVERT_F_TO_S: u16 = 110;
const OP_CONVERT_U_TO_F: u16 = 112;
const OP_BITCAST: u16 = 124;
const OP_IADD: u16 = 128;
const OP_FADD: u16 = 129;
const OP_ISUB: u16 = 130;
const OP_IMUL: u16 = 132;
const OP_FMUL: u16 = 133;
const OP_UDIV: u16 = 134;
const OP_UMOD: u16 = 137;
const OP_LOGICAL_NOT: u16 = 168;
const OP_SELECT: u16 = 169;
const OP_IEQUAL: u16 = 170;
const OP_INOT_EQUAL: u16 = 171;
const OP_UGREATER_THAN: u16 = 172;
const OP_SGREATER_THAN_EQUAL: u16 = 175;
const OP_UGREATER_THAN_EQUAL: u16 = 174;
const OP_ULESS_THAN: u16 = 176;
const OP_SLESS_THAN: u16 = 177;
const OP_SLESS_THAN_EQUAL: u16 = 179;
const OP_FORD_LESS_THAN_EQUAL: u16 = 188;
const OP_SHIFT_RIGHT_LOGICAL: u16 = 194;
const OP_SHIFT_LEFT_LOGICAL: u16 = 196;
const OP_BITWISE_OR: u16 = 197;
const OP_BITWISE_AND: u16 = 199;
const OP_PHI: u16 = 245;
const OP_LOOP_MERGE: u16 = 246;
const OP_SELECTION_MERGE: u16 = 247;
const OP_LABEL: u16 = 248;
const OP_BRANCH: u16 = 249;
const OP_BRANCH_CONDITIONAL: u16 = 250;
const OP_SWITCH: u16 = 251;
const OP_RETURN: u16 = 253;

const GLSL_POW: u32 = 26;
const GLSL_UMIN: u32 = 38;
const GLSL_UMAX: u32 = 41;
const GLSL_SMAX: u32 = 42;

struct WordReader {
    words: Vec<u32>,
    pos: usize,
}

impl WordReader {
    fn new(data: &[u8]) -> Result<Self, Error> {
        if !data.len().is_multiple_of(4) {
            return Err(Error::new("SPIR-V data length is not a multiple of 4"));
        }
        let word_count = data.len() / 4;
        let mut words = vec![0u32; word_count];
        for (i, chunk) in data.chunks_exact(4).enumerate() {
            words[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        Ok(Self { words, pos: 0 })
    }

    fn remaining(&self) -> usize {
        self.words.len() - self.pos
    }

    fn read(&mut self) -> Result<u32, Error> {
        if self.pos >= self.words.len() {
            return Err(Error::new("unexpected end of SPIR-V data"));
        }
        let word = self.words[self.pos];
        self.pos += 1;
        Ok(word)
    }

    fn read_string(&mut self, word_count: usize) -> Result<String, Error> {
        let mut bytes = Vec::new();
        for _ in 0..word_count {
            let word = self.read()?;
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8(bytes[..end].to_vec())
            .map_err(|e| Error::new(format!("invalid UTF-8 in SPIR-V string: {e}")))
    }
}

fn parse_storage_class(value: u32) -> Result<StorageClass, Error> {
    match value {
        1 => Ok(StorageClass::Input),
        3 => Ok(StorageClass::Output),
        7 => Ok(StorageClass::Function),
        12 => Ok(StorageClass::StorageBuffer),
        _ => Err(Error::new(format!("unsupported storage class: {value}"))),
    }
}

pub(crate) fn parse(spirv_bytes: &[u8]) -> Result<Module, Error> {
    let mut reader = WordReader::new(spirv_bytes)?;

    let magic = reader.read()?;
    if magic != SPIRV_MAGIC {
        return Err(Error::new(format!(
            "invalid SPIR-V magic: 0x{magic:08X}, expected 0x{SPIRV_MAGIC:08X}"
        )));
    }
    let _version = reader.read()?;
    let _generator = reader.read()?;
    let _bound = reader.read()?;
    let _schema = reader.read()?;

    let mut entry_points = Vec::new();
    let mut types: HashMap<u32, Type> = HashMap::new();
    let mut constants: HashMap<u32, ConstantValue> = HashMap::new();
    let mut global_variables: HashMap<u32, GlobalVariable> = HashMap::new();
    let mut decorations: HashMap<u32, Vec<Decoration>> = HashMap::new();
    let mut member_decorations: HashMap<(u32, u32), Vec<Decoration>> = HashMap::new();
    let mut functions: Vec<Function> = Vec::new();

    let mut current_function: Option<Function> = None;
    let mut current_block: Option<Block> = None;

    while reader.remaining() > 0 {
        let instruction_word = reader.read()?;
        let word_count = (instruction_word >> 16) as usize;
        let opcode = (instruction_word & 0xFFFF) as u16;

        if word_count == 0 {
            return Err(Error::new("zero word count in SPIR-V instruction"));
        }

        let words_to_read = word_count - 1;
        let start_pos = reader.pos;

        match opcode {
            OP_SOURCE | OP_NAME | OP_MEMBER_NAME | OP_EXTENSION | OP_MEMORY_MODEL
            | OP_CAPABILITY | OP_EXECUTION_MODE => {
                for _ in 0..words_to_read {
                    reader.read()?;
                }
            }

            OP_EXT_INST_IMPORT => {
                for _ in 0..words_to_read {
                    reader.read()?;
                }
            }

            OP_ENTRY_POINT => {
                let exec_model_raw = reader.read()?;
                let function_id = reader.read()?;
                let name_start = reader.pos;
                let remaining_words = words_to_read - 2;
                let mut name_words = 0;
                for i in 0..remaining_words {
                    let word = reader.words[name_start + i];
                    name_words += 1;
                    let bytes = word.to_le_bytes();
                    if bytes.contains(&0) {
                        break;
                    }
                }
                reader.pos = name_start;
                let name = reader.read_string(name_words)?;
                let interface_words = remaining_words - name_words;
                let mut interface_ids = Vec::with_capacity(interface_words);
                for _ in 0..interface_words {
                    interface_ids.push(reader.read()?);
                }
                let execution_model = match exec_model_raw {
                    0 => ExecutionModel::Vertex,
                    4 => ExecutionModel::Fragment,
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported execution model: {exec_model_raw}"
                        )));
                    }
                };
                entry_points.push(EntryPoint {
                    execution_model,
                    function_id,
                    name,
                    interface_ids,
                });
            }

            OP_TYPE_VOID => {
                let result_id = reader.read()?;
                types.insert(result_id, Type::Void);
            }

            OP_TYPE_BOOL => {
                let result_id = reader.read()?;
                types.insert(result_id, Type::Bool);
            }

            OP_TYPE_INT => {
                let result_id = reader.read()?;
                let width = reader.read()?;
                let _signedness = reader.read()?;
                types.insert(result_id, Type::Int { width });
            }

            OP_TYPE_FLOAT => {
                let result_id = reader.read()?;
                let width = reader.read()?;
                types.insert(result_id, Type::Float { width });
            }

            OP_TYPE_VECTOR => {
                let result_id = reader.read()?;
                let component_type_id = reader.read()?;
                let count = reader.read()?;
                types.insert(
                    result_id,
                    Type::Vector {
                        component_type_id,
                        count,
                    },
                );
            }

            OP_TYPE_ARRAY => {
                let result_id = reader.read()?;
                let element_type_id = reader.read()?;
                let _length_id = reader.read()?;
                types.insert(result_id, Type::Array { element_type_id });
            }

            OP_TYPE_RUNTIME_ARRAY => {
                let result_id = reader.read()?;
                let element_type_id = reader.read()?;
                types.insert(result_id, Type::RuntimeArray { element_type_id });
            }

            OP_TYPE_STRUCT => {
                let result_id = reader.read()?;
                let member_count = words_to_read - 1;
                let mut member_type_ids = Vec::with_capacity(member_count);
                for _ in 0..member_count {
                    member_type_ids.push(reader.read()?);
                }
                types.insert(result_id, Type::Struct { member_type_ids });
            }

            OP_TYPE_POINTER => {
                let result_id = reader.read()?;
                let _storage_class_raw = reader.read()?;
                let pointee_type_id = reader.read()?;
                types.insert(result_id, Type::Pointer { pointee_type_id });
            }

            OP_TYPE_FUNCTION => {
                let result_id = reader.read()?;
                for _ in 0..(words_to_read - 1) {
                    reader.read()?;
                }
                types.insert(result_id, Type::Function);
            }

            OP_CONSTANT_TRUE => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                constants.insert(result_id, ConstantValue::Bool(true));
            }

            OP_CONSTANT_FALSE => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                constants.insert(result_id, ConstantValue::Bool(false));
            }

            OP_CONSTANT => {
                let result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let value_word = reader.read()?;
                let value = match &types[&result_type_id] {
                    Type::Int { .. } => ConstantValue::U32(value_word),
                    Type::Float { .. } => ConstantValue::F32(f32::from_bits(value_word)),
                    _ => {
                        return Err(Error::new(format!(
                            "unexpected type for OpConstant: {result_type_id}"
                        )));
                    }
                };
                constants.insert(result_id, value);
            }

            OP_CONSTANT_COMPOSITE => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let constituent_count = words_to_read - 2;
                let mut constituent_ids = Vec::with_capacity(constituent_count);
                for _ in 0..constituent_count {
                    constituent_ids.push(reader.read()?);
                }
                constants.insert(result_id, ConstantValue::Composite(constituent_ids));
            }

            OP_DECORATE => {
                let target_id = reader.read()?;
                let decoration_value = reader.read()?;
                let decoration = match decoration_value {
                    2 => Some(Decoration::Block),
                    6 => {
                        let stride = reader.read()?;
                        Some(Decoration::ArrayStride(stride))
                    }
                    11 => {
                        let builtin_value = reader.read()?;
                        let builtin = match builtin_value {
                            0 => BuiltIn::Position,
                            15 => BuiltIn::FragCoord,
                            42 => BuiltIn::VertexIndex,
                            _ => {
                                return Err(Error::new(format!(
                                    "unsupported BuiltIn: {builtin_value}"
                                )));
                            }
                        };
                        Some(Decoration::BuiltIn(builtin))
                    }
                    18 => Some(Decoration::NonWritable),
                    30 => {
                        let location = reader.read()?;
                        Some(Decoration::Location(location))
                    }
                    33 => {
                        let binding = reader.read()?;
                        Some(Decoration::Binding(binding))
                    }
                    _ => {
                        let consumed = reader.pos - start_pos;
                        for _ in consumed..words_to_read {
                            reader.read()?;
                        }
                        None
                    }
                };
                if let Some(dec) = decoration {
                    decorations.entry(target_id).or_default().push(dec);
                }
            }

            OP_MEMBER_DECORATE => {
                let struct_type_id = reader.read()?;
                let member_index = reader.read()?;
                let decoration_value = reader.read()?;
                let decoration = match decoration_value {
                    35 => {
                        let offset = reader.read()?;
                        Some(Decoration::Offset(offset))
                    }
                    _ => {
                        let consumed = reader.pos - start_pos;
                        for _ in consumed..words_to_read {
                            reader.read()?;
                        }
                        None
                    }
                };
                if let Some(dec) = decoration {
                    member_decorations
                        .entry((struct_type_id, member_index))
                        .or_default()
                        .push(dec);
                }
            }

            OP_VARIABLE => {
                let result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let storage_class_raw = reader.read()?;
                let storage_class = parse_storage_class(storage_class_raw)?;

                if current_function.is_some() {
                    push_instruction(&mut current_block, Instruction::Variable { result_id })?;
                } else {
                    global_variables.insert(
                        result_id,
                        GlobalVariable {
                            type_id: result_type_id,
                            storage_class,
                        },
                    );
                }
                let consumed = reader.pos - start_pos;
                for _ in consumed..words_to_read {
                    reader.read()?;
                }
            }

            OP_FUNCTION => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let _function_control = reader.read()?;
                let _function_type_id = reader.read()?;
                current_function = Some(Function {
                    result_id,
                    blocks: Vec::new(),
                });
            }

            OP_FUNCTION_END => {
                if let Some(mut block) = current_block.take() {
                    if !matches!(block.terminator, Terminator::Return) {
                        block.terminator = Terminator::Return;
                    }
                    if let Some(ref mut func) = current_function {
                        func.blocks.push(block);
                    }
                }
                if let Some(func) = current_function.take() {
                    functions.push(func);
                }
            }

            OP_LABEL => {
                let label_id = reader.read()?;
                if let (Some(block), Some(func)) = (current_block.take(), &mut current_function) {
                    func.blocks.push(block);
                }
                current_block = Some(Block {
                    label_id,
                    phi_instructions: Vec::new(),
                    instructions: Vec::new(),
                    terminator: Terminator::Return,
                });
            }

            OP_PHI => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let pair_count = (words_to_read - 2) / 2;
                let mut incoming = Vec::with_capacity(pair_count);
                for _ in 0..pair_count {
                    let value_id = reader.read()?;
                    let block_label_id = reader.read()?;
                    incoming.push((value_id, block_label_id));
                }
                if let Some(ref mut block) = current_block {
                    block.phi_instructions.push(Phi {
                        result_id,
                        incoming,
                    });
                }
            }

            OP_BRANCH => {
                let target_label = reader.read()?;
                if let Some(ref mut block) = current_block {
                    block.terminator = Terminator::Branch { target_label };
                }
            }

            OP_BRANCH_CONDITIONAL => {
                let condition_id = reader.read()?;
                let true_label = reader.read()?;
                let false_label = reader.read()?;
                let consumed = reader.pos - start_pos;
                for _ in consumed..words_to_read {
                    reader.read()?;
                }
                if let Some(ref mut block) = current_block {
                    block.terminator = Terminator::BranchConditional {
                        condition_id,
                        true_label,
                        false_label,
                    };
                }
            }

            OP_SWITCH => {
                let selector_id = reader.read()?;
                let default_label = reader.read()?;
                let target_count = (words_to_read - 2) / 2;
                let mut targets = Vec::with_capacity(target_count);
                for _ in 0..target_count {
                    let literal = reader.read()?;
                    let label = reader.read()?;
                    targets.push((literal, label));
                }
                if let Some(ref mut block) = current_block {
                    block.terminator = Terminator::Switch {
                        selector_id,
                        default_label,
                        targets,
                    };
                }
            }

            OP_RETURN => {
                if let Some(ref mut block) = current_block {
                    block.terminator = Terminator::Return;
                }
            }

            OP_SELECTION_MERGE => {
                let _merge_block = reader.read()?;
                let _selection_control = reader.read()?;
                push_instruction(&mut current_block, Instruction::SelectionMerge)?;
            }

            OP_LOOP_MERGE => {
                let _merge_block = reader.read()?;
                let _continue_target = reader.read()?;
                let _loop_control = reader.read()?;
                let consumed = reader.pos - start_pos;
                for _ in consumed..words_to_read {
                    reader.read()?;
                }
                push_instruction(&mut current_block, Instruction::LoopMerge)?;
            }

            OP_LOAD => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let pointer_id = reader.read()?;
                let consumed = reader.pos - start_pos;
                for _ in consumed..words_to_read {
                    reader.read()?;
                }
                push_instruction(
                    &mut current_block,
                    Instruction::Load {
                        result_id,
                        pointer_id,
                    },
                )?;
            }

            OP_STORE => {
                let pointer_id = reader.read()?;
                let value_id = reader.read()?;
                let consumed = reader.pos - start_pos;
                for _ in consumed..words_to_read {
                    reader.read()?;
                }
                push_instruction(
                    &mut current_block,
                    Instruction::Store {
                        pointer_id,
                        value_id,
                    },
                )?;
            }

            OP_ACCESS_CHAIN => {
                let result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let base_id = reader.read()?;
                let index_count = words_to_read - 3;
                let mut indexes = Vec::with_capacity(index_count);
                for _ in 0..index_count {
                    indexes.push(reader.read()?);
                }
                push_instruction(
                    &mut current_block,
                    Instruction::AccessChain {
                        result_id,
                        result_type_id,
                        base_id,
                        indexes,
                    },
                )?;
            }

            OP_VECTOR_SHUFFLE => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let vector1_id = reader.read()?;
                let vector2_id = reader.read()?;
                let component_count = words_to_read - 4;
                let mut components = Vec::with_capacity(component_count);
                for _ in 0..component_count {
                    components.push(reader.read()?);
                }
                push_instruction(
                    &mut current_block,
                    Instruction::VectorShuffle {
                        result_id,
                        vector1_id,
                        vector2_id,
                        components,
                    },
                )?;
            }

            OP_COMPOSITE_CONSTRUCT => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let count = words_to_read - 2;
                let mut constituent_ids = Vec::with_capacity(count);
                for _ in 0..count {
                    constituent_ids.push(reader.read()?);
                }
                push_instruction(
                    &mut current_block,
                    Instruction::CompositeConstruct {
                        result_id,
                        constituent_ids,
                    },
                )?;
            }

            OP_COMPOSITE_EXTRACT => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let composite_id = reader.read()?;
                let count = words_to_read - 3;
                let mut indexes = Vec::with_capacity(count);
                for _ in 0..count {
                    indexes.push(reader.read()?);
                }
                push_instruction(
                    &mut current_block,
                    Instruction::CompositeExtract {
                        result_id,
                        composite_id,
                        indexes,
                    },
                )?;
            }

            OP_EXT_INST => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let _set_id = reader.read()?;
                let inst = reader.read()?;
                let operand_count = words_to_read - 4;
                let mut operand_ids = Vec::with_capacity(operand_count);
                for _ in 0..operand_count {
                    operand_ids.push(reader.read()?);
                }
                let instruction = match inst {
                    GLSL_POW => GlslExtInst::Pow,
                    GLSL_UMIN => GlslExtInst::UMin,
                    GLSL_UMAX => GlslExtInst::UMax,
                    GLSL_SMAX => GlslExtInst::SMax,
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported GLSL.std.450 instruction: {inst}"
                        )));
                    }
                };
                push_instruction(
                    &mut current_block,
                    Instruction::ExtInst {
                        result_id,
                        instruction,
                        operand_ids,
                    },
                )?;
            }

            OP_CONVERT_F_TO_S => {
                parse_unary_op(&mut reader, &mut current_block, |result_id, operand_id| {
                    Instruction::ConvertFToS {
                        result_id,
                        operand_id,
                    }
                })?
            }

            OP_CONVERT_U_TO_F => {
                parse_unary_op(&mut reader, &mut current_block, |result_id, operand_id| {
                    Instruction::ConvertUToF {
                        result_id,
                        operand_id,
                    }
                })?
            }

            OP_BITCAST => {
                let result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let operand_id = reader.read()?;
                push_instruction(
                    &mut current_block,
                    Instruction::Bitcast {
                        result_id,
                        result_type_id,
                        operand_id,
                    },
                )?;
            }

            OP_LOGICAL_NOT => {
                parse_unary_op(&mut reader, &mut current_block, |result_id, operand_id| {
                    Instruction::LogicalNot {
                        result_id,
                        operand_id,
                    }
                })?
            }

            OP_IADD => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::IAdd {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_FADD => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::FAdd {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_ISUB => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::ISub {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_IMUL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::IMul {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_FMUL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::FMul {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_UDIV => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::UDiv {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_UMOD => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::UMod {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_BITWISE_AND => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::BitwiseAnd {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_BITWISE_OR => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::BitwiseOr {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_SHIFT_RIGHT_LOGICAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, base_id, shift_id| Instruction::ShiftRightLogical {
                    result_id,
                    base_id,
                    shift_id,
                },
            )?,
            OP_SHIFT_LEFT_LOGICAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, base_id, shift_id| Instruction::ShiftLeftLogical {
                    result_id,
                    base_id,
                    shift_id,
                },
            )?,
            OP_IEQUAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::IEqual {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_INOT_EQUAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::INotEqual {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_UGREATER_THAN => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::UGreaterThan {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_UGREATER_THAN_EQUAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::UGreaterThanEqual {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_SGREATER_THAN_EQUAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::SGreaterThanEqual {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_ULESS_THAN => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::ULessThan {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_SLESS_THAN => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::SLessThan {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_SLESS_THAN_EQUAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::SLessThanEqual {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,
            OP_FORD_LESS_THAN_EQUAL => parse_binary_op(
                &mut reader,
                &mut current_block,
                |result_id, left_id, right_id| Instruction::FOrdLessThanEqual {
                    result_id,
                    left_id,
                    right_id,
                },
            )?,

            OP_SELECT => {
                let _result_type_id = reader.read()?;
                let result_id = reader.read()?;
                let condition_id = reader.read()?;
                let true_id = reader.read()?;
                let false_id = reader.read()?;
                push_instruction(
                    &mut current_block,
                    Instruction::Select {
                        result_id,
                        condition_id,
                        true_id,
                        false_id,
                    },
                )?;
            }

            _ => {
                return Err(Error::new(format!("unsupported SPIR-V opcode: {opcode}")));
            }
        }

        let consumed = reader.pos - start_pos;
        if consumed != words_to_read {
            return Err(Error::new(format!(
                "opcode {opcode}: consumed {consumed} words but expected {words_to_read}"
            )));
        }
    }

    Ok(Module {
        entry_points,
        types,
        constants,
        global_variables,
        functions,
        decorations,
        member_decorations,
    })
}

fn push_instruction(
    current_block: &mut Option<Block>,
    instruction: Instruction,
) -> Result<(), Error> {
    match current_block {
        Some(block) => {
            block.instructions.push(instruction);
            Ok(())
        }
        None => Err(Error::new("instruction outside of a block")),
    }
}

fn parse_unary_op(
    reader: &mut WordReader,
    current_block: &mut Option<Block>,
    make: impl FnOnce(u32, u32) -> Instruction,
) -> Result<(), Error> {
    let _result_type_id = reader.read()?;
    let result_id = reader.read()?;
    let operand_id = reader.read()?;
    push_instruction(current_block, make(result_id, operand_id))
}

fn parse_binary_op(
    reader: &mut WordReader,
    current_block: &mut Option<Block>,
    make: impl FnOnce(u32, u32, u32) -> Instruction,
) -> Result<(), Error> {
    let _result_type_id = reader.read()?;
    let result_id = reader.read()?;
    let left_id = reader.read()?;
    let right_id = reader.read()?;
    push_instruction(current_block, make(result_id, left_id, right_id))
}
