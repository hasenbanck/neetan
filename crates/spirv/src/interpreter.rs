use std::collections::HashMap;

use crate::{
    error::Error,
    module::{
        BuiltIn, Decoration, GlslExtInst, Instruction, Module, StorageClass, Terminator, Type,
        Value as ConstantValue,
    },
    value::{Pointer, Value},
};

pub(crate) struct Interpreter<'a> {
    module: &'a Module,
    interface_ids: Vec<u32>,
    values: HashMap<u32, Value>,
    output_values: HashMap<u32, Value>,
    display_buffer: &'a [u32],
    font_rom_buffer: &'a [u32],
    pegc_buffer: &'a [u32],
}

impl<'a> Interpreter<'a> {
    pub(crate) fn new(
        module: &'a Module,
        interface_ids: &[u32],
        display_buffer: &'a [u32],
        font_rom_buffer: &'a [u32],
        pegc_buffer: &'a [u32],
    ) -> Self {
        Self {
            module,
            interface_ids: interface_ids.to_vec(),
            values: HashMap::new(),
            output_values: HashMap::new(),
            display_buffer,
            font_rom_buffer,
            pegc_buffer,
        }
    }

    pub(crate) fn execute_fragment(
        &mut self,
        function_index: usize,
        frag_coord: Value,
    ) -> Result<Value, Error> {
        self.values.clear();
        self.output_values.clear();

        self.preload_constants()?;
        self.setup_globals(frag_coord)?;

        let function = &self.module.functions[function_index];
        let blocks = &function.blocks;

        let mut block_index_map: HashMap<u32, usize> = HashMap::new();
        for (i, block) in blocks.iter().enumerate() {
            block_index_map.insert(block.label_id, i);
        }

        let mut current_block_index = 0;
        let mut predecessor_label: Option<u32> = None;

        loop {
            let block = &blocks[current_block_index];

            // Evaluate Phi nodes: read all incoming values first, then write results.
            let phi_results: Vec<(u32, Value)> = block
                .phi_instructions
                .iter()
                .map(|phi| {
                    let pred = predecessor_label
                        .ok_or_else(|| Error::new("OpPhi in entry block with no predecessor"))?;
                    let (value_id, _) = phi
                        .incoming
                        .iter()
                        .find(|(_, label)| *label == pred)
                        .ok_or_else(|| {
                            Error::new(format!(
                                "OpPhi %{}: no incoming for predecessor %{pred}",
                                phi.result_id
                            ))
                        })?;
                    let value = self.get_value(*value_id)?;
                    Ok((phi.result_id, value))
                })
                .collect::<Result<Vec<_>, Error>>()?;

            for (id, value) in phi_results {
                self.values.insert(id, value);
            }

            for instruction in &block.instructions {
                self.execute_instruction(instruction)?;
            }

            predecessor_label = Some(block.label_id);

            match &block.terminator {
                Terminator::Return => {
                    return self.get_fragment_output();
                }
                Terminator::Branch { target_label } => {
                    current_block_index = block_index_map[target_label];
                }
                Terminator::BranchConditional {
                    condition_id,
                    true_label,
                    false_label,
                } => {
                    let condition = self.get_value(*condition_id)?.as_bool();
                    let target = if condition { *true_label } else { *false_label };
                    current_block_index = block_index_map[&target];
                }
                Terminator::Switch {
                    selector_id,
                    default_label,
                    targets,
                } => {
                    let selector = self.get_value(*selector_id)?.as_u32();
                    let target = targets
                        .iter()
                        .find(|(literal, _)| *literal == selector)
                        .map(|(_, label)| *label)
                        .unwrap_or(*default_label);
                    current_block_index = block_index_map[&target];
                }
            }
        }
    }

    fn preload_constants(&mut self) -> Result<(), Error> {
        for (&id, constant) in &self.module.constants {
            let value = match constant {
                ConstantValue::Bool(b) => Value::Bool(*b),
                ConstantValue::U32(v) => Value::U32(*v),
                ConstantValue::F32(v) => Value::F32(*v),
                ConstantValue::Composite(_) => continue,
            };
            self.values.insert(id, value);
        }
        // Resolve composite constants (they reference other constants).
        for (&id, constant) in &self.module.constants {
            if let ConstantValue::Composite(constituent_ids) = constant {
                let components: Vec<Value> = constituent_ids
                    .iter()
                    .map(|cid| self.get_value(*cid))
                    .collect::<Result<_, _>>()?;
                self.values.insert(id, Value::Vector(components));
            }
        }
        Ok(())
    }

    fn setup_globals(&mut self, frag_coord: Value) -> Result<(), Error> {
        for (&id, var) in &self.module.global_variables {
            if !self.interface_ids.contains(&id) && var.storage_class != StorageClass::StorageBuffer
            {
                continue;
            }
            match var.storage_class {
                StorageClass::Input => {
                    let decorations = self.module.decorations.get(&id);
                    let is_frag_coord = decorations.is_some_and(|decs| {
                        decs.iter()
                            .any(|d| matches!(d, Decoration::BuiltIn(BuiltIn::FragCoord)))
                    });
                    if is_frag_coord {
                        self.values
                            .insert(id, Value::Pointer(Pointer::Input { variable_id: id }));
                        self.output_values.insert(id, frag_coord.clone());
                    } else {
                        // Location 0 input (uv) - not used by compose, provide dummy.
                        self.values
                            .insert(id, Value::Pointer(Pointer::Input { variable_id: id }));
                        self.output_values
                            .insert(id, Value::Vector(vec![Value::F32(0.0), Value::F32(0.0)]));
                    }
                }
                StorageClass::Output => {
                    self.values
                        .insert(id, Value::Pointer(Pointer::Output { variable_id: id }));
                }
                StorageClass::StorageBuffer => {
                    let binding = self.get_binding(id)?;
                    let pointee_type_id = if let Type::Pointer {
                        pointee_type_id, ..
                    } = &self.module.types[&var.type_id]
                    {
                        *pointee_type_id
                    } else {
                        return Err(Error::new("StorageBuffer variable is not a pointer type"));
                    };
                    self.values.insert(
                        id,
                        Value::Pointer(Pointer::StorageBuffer {
                            binding,
                            byte_offset: 0,
                            pointee_type_id,
                        }),
                    );
                }
                StorageClass::Function => {}
            }
        }
        Ok(())
    }

    fn get_binding(&self, variable_id: u32) -> Result<u32, Error> {
        let decorations = self
            .module
            .decorations
            .get(&variable_id)
            .ok_or_else(|| Error::new(format!("no decorations for variable %{variable_id}")))?;
        for dec in decorations {
            if let Decoration::Binding(b) = dec {
                return Ok(*b);
            }
        }
        Err(Error::new(format!(
            "no Binding decoration for variable %{variable_id}"
        )))
    }

    fn get_value(&self, id: u32) -> Result<Value, Error> {
        self.values
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::new(format!("undefined value %{id}")))
    }

    fn get_fragment_output(&self) -> Result<Value, Error> {
        for (&id, var) in &self.module.global_variables {
            if var.storage_class == StorageClass::Output && self.interface_ids.contains(&id) {
                let decorations = self.module.decorations.get(&id);
                let is_location_0 = decorations
                    .is_some_and(|decs| decs.iter().any(|d| matches!(d, Decoration::Location(0))));
                if is_location_0 {
                    return self
                        .output_values
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| Error::new("fragment output was never written"));
                }
            }
        }
        Err(Error::new("no fragment output variable found"))
    }

    fn execute_instruction(&mut self, instruction: &Instruction) -> Result<(), Error> {
        match instruction {
            Instruction::SelectionMerge | Instruction::LoopMerge => {
                // Structured control flow annotations - no runtime effect.
            }

            Instruction::Variable { result_id } => {
                // Function-local variable - should not appear in compose shader,
                // but handle it as a local mutable slot just in case.
                self.output_values.insert(*result_id, Value::Bool(false));
                self.values.insert(
                    *result_id,
                    Value::Pointer(Pointer::Output {
                        variable_id: *result_id,
                    }),
                );
            }

            Instruction::Load {
                result_id,
                pointer_id,
            } => {
                let pointer = self.get_value(*pointer_id)?;
                let value = match pointer.as_pointer() {
                    Pointer::StorageBuffer {
                        binding,
                        byte_offset,
                        ..
                    } => {
                        let buffer = self.get_buffer(*binding)?;
                        let word_index = (*byte_offset / 4) as usize;
                        if word_index >= buffer.len() {
                            return Err(Error::new(format!(
                                "storage buffer read out of bounds: binding={binding}, offset={byte_offset}, word_index={word_index}, buffer_len={}",
                                buffer.len()
                            )));
                        }
                        Value::U32(buffer[word_index])
                    }
                    Pointer::Output { variable_id } => self
                        .output_values
                        .get(variable_id)
                        .cloned()
                        .ok_or_else(|| {
                            Error::new(format!("read from uninitialized variable %{variable_id}"))
                        })?,
                    Pointer::Input { variable_id } => self
                        .output_values
                        .get(variable_id)
                        .cloned()
                        .ok_or_else(|| {
                            Error::new(format!("read from uninitialized input %{variable_id}"))
                        })?,
                };
                self.values.insert(*result_id, value);
            }

            Instruction::Store {
                pointer_id,
                value_id,
            } => {
                let pointer = self.get_value(*pointer_id)?;
                let value = self.get_value(*value_id)?;
                match pointer.as_pointer() {
                    Pointer::Output { variable_id } => {
                        self.output_values.insert(*variable_id, value);
                    }
                    Pointer::Input { .. } => {
                        return Err(Error::new("cannot write to input variable"));
                    }
                    Pointer::StorageBuffer { .. } => {
                        return Err(Error::new("cannot write to storage buffer (NonWritable)"));
                    }
                }
            }

            Instruction::AccessChain {
                result_id,
                result_type_id,
                base_id,
                indexes,
            } => {
                let base = self.get_value(*base_id)?;
                let pointer =
                    self.evaluate_access_chain(base.as_pointer(), *result_type_id, indexes)?;
                self.values.insert(*result_id, Value::Pointer(pointer));
            }

            Instruction::VectorShuffle {
                result_id,
                vector1_id,
                vector2_id,
                components,
            } => {
                let v1 = self.get_value(*vector1_id)?;
                let v2 = self.get_value(*vector2_id)?;
                let v1_components = v1.as_vector();
                let v2_components = v2.as_vector();
                let v1_len = v1_components.len() as u32;
                let result: Vec<Value> = components
                    .iter()
                    .map(|&c| {
                        if c < v1_len {
                            v1_components[c as usize].clone()
                        } else {
                            v2_components[(c - v1_len) as usize].clone()
                        }
                    })
                    .collect();
                self.values.insert(*result_id, Value::Vector(result));
            }

            Instruction::CompositeConstruct {
                result_id,
                constituent_ids,
            } => {
                let components: Vec<Value> = constituent_ids
                    .iter()
                    .map(|id| self.get_value(*id))
                    .collect::<Result<_, _>>()?;
                self.values.insert(*result_id, Value::Vector(components));
            }

            Instruction::CompositeExtract {
                result_id,
                composite_id,
                indexes,
            } => {
                let composite = self.get_value(*composite_id)?;
                let mut current = composite;
                for &index in indexes {
                    current = current.as_vector()[index as usize].clone();
                }
                self.values.insert(*result_id, current);
            }

            Instruction::ConvertFToS {
                result_id,

                operand_id,
            } => {
                let operand = self.get_value(*operand_id)?;
                let result = match &operand {
                    Value::F32(v) => Value::U32(*v as i32 as u32),
                    Value::Vector(components) => {
                        let converted: Vec<Value> = components
                            .iter()
                            .map(|c| Value::U32(c.as_f32() as i32 as u32))
                            .collect();
                        Value::Vector(converted)
                    }
                    _ => return Err(Error::new("ConvertFToS: unsupported operand type")),
                };
                self.values.insert(*result_id, result);
            }

            Instruction::ConvertUToF {
                result_id,

                operand_id,
            } => {
                let operand = self.get_value(*operand_id)?;
                let result = match &operand {
                    Value::U32(v) => Value::F32(*v as f32),
                    Value::Vector(components) => {
                        let converted: Vec<Value> = components
                            .iter()
                            .map(|c| Value::U32(c.as_u32()))
                            .map(|c| Value::F32(c.as_u32() as f32))
                            .collect();
                        Value::Vector(converted)
                    }
                    _ => return Err(Error::new("ConvertUToF: unsupported operand type")),
                };
                self.values.insert(*result_id, result);
            }

            Instruction::Bitcast {
                result_id,
                result_type_id,
                operand_id,
            } => {
                let operand = self.get_value(*operand_id)?;
                let result = self.bitcast(*result_type_id, &operand)?;
                self.values.insert(*result_id, result);
            }

            Instruction::IAdd {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values
                    .insert(*result_id, Value::U32(left.wrapping_add(right)));
            }

            Instruction::FAdd {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_f32();
                let right = self.get_value(*right_id)?.as_f32();
                self.values.insert(*result_id, Value::F32(left + right));
            }

            Instruction::ISub {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values
                    .insert(*result_id, Value::U32(left.wrapping_sub(right)));
            }

            Instruction::IMul {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values
                    .insert(*result_id, Value::U32(left.wrapping_mul(right)));
            }

            Instruction::FMul {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_f32();
                let right = self.get_value(*right_id)?.as_f32();
                self.values.insert(*result_id, Value::F32(left * right));
            }

            Instruction::UDiv {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                let result = if right == 0 { 0 } else { left / right };
                self.values.insert(*result_id, Value::U32(result));
            }

            Instruction::UMod {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                let result = if right == 0 { 0 } else { left % right };
                self.values.insert(*result_id, Value::U32(result));
            }

            Instruction::BitwiseAnd {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::U32(left & right));
            }

            Instruction::BitwiseOr {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::U32(left | right));
            }

            Instruction::ShiftRightLogical {
                result_id,

                base_id,
                shift_id,
            } => {
                let base = self.get_value(*base_id)?.as_u32();
                let shift = self.get_value(*shift_id)?.as_u32();
                self.values
                    .insert(*result_id, Value::U32(base >> (shift & 31)));
            }

            Instruction::ShiftLeftLogical {
                result_id,

                base_id,
                shift_id,
            } => {
                let base = self.get_value(*base_id)?.as_u32();
                let shift = self.get_value(*shift_id)?.as_u32();
                self.values
                    .insert(*result_id, Value::U32(base << (shift & 31)));
            }

            Instruction::IEqual {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::Bool(left == right));
            }

            Instruction::INotEqual {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::Bool(left != right));
            }

            Instruction::UGreaterThan {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::Bool(left > right));
            }

            Instruction::UGreaterThanEqual {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::Bool(left >= right));
            }

            Instruction::SGreaterThanEqual {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32() as i32;
                let right = self.get_value(*right_id)?.as_u32() as i32;
                self.values.insert(*result_id, Value::Bool(left >= right));
            }

            Instruction::ULessThan {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32();
                let right = self.get_value(*right_id)?.as_u32();
                self.values.insert(*result_id, Value::Bool(left < right));
            }

            Instruction::SLessThan {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32() as i32;
                let right = self.get_value(*right_id)?.as_u32() as i32;
                self.values.insert(*result_id, Value::Bool(left < right));
            }

            Instruction::SLessThanEqual {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_u32() as i32;
                let right = self.get_value(*right_id)?.as_u32() as i32;
                self.values.insert(*result_id, Value::Bool(left <= right));
            }

            Instruction::FOrdLessThanEqual {
                result_id,

                left_id,
                right_id,
            } => {
                let left = self.get_value(*left_id)?.as_f32();
                let right = self.get_value(*right_id)?.as_f32();
                self.values.insert(*result_id, Value::Bool(left <= right));
            }

            Instruction::LogicalNot {
                result_id,

                operand_id,
            } => {
                let operand = self.get_value(*operand_id)?.as_bool();
                self.values.insert(*result_id, Value::Bool(!operand));
            }

            Instruction::Select {
                result_id,

                condition_id,
                true_id,
                false_id,
            } => {
                let condition = self.get_value(*condition_id)?.as_bool();
                let result = if condition {
                    self.get_value(*true_id)?
                } else {
                    self.get_value(*false_id)?
                };
                self.values.insert(*result_id, result);
            }

            Instruction::ExtInst {
                result_id,

                instruction,
                operand_ids,
            } => {
                let result = self.execute_glsl_ext(*instruction, operand_ids)?;
                self.values.insert(*result_id, result);
            }
        }

        Ok(())
    }

    fn evaluate_access_chain(
        &self,
        base: &Pointer,
        result_type_id: u32,
        indexes: &[u32],
    ) -> Result<Pointer, Error> {
        match base {
            Pointer::StorageBuffer {
                binding,
                byte_offset,
                pointee_type_id,
            } => {
                let mut offset = *byte_offset;
                let mut current_type_id = *pointee_type_id;

                for &index_id in indexes {
                    let index = self.get_value(index_id)?.as_u32();
                    match &self.module.types[&current_type_id] {
                        Type::RuntimeArray { element_type_id } => {
                            let stride = self.get_type_array_stride(current_type_id)?;
                            offset += index * stride;
                            current_type_id = *element_type_id;
                        }
                        Type::Struct { member_type_ids } => {
                            let member_offset = self.get_member_offset(current_type_id, index)?;
                            offset += member_offset;
                            current_type_id = member_type_ids[index as usize];
                        }
                        Type::Array {
                            element_type_id, ..
                        } => {
                            let stride = self.get_type_array_stride(current_type_id)?;
                            offset += index * stride;
                            current_type_id = *element_type_id;
                        }
                        _ => {
                            return Err(Error::new(format!(
                                "AccessChain into unsupported type: {:?}",
                                self.module.types[&current_type_id]
                            )));
                        }
                    }
                }

                // The result_type_id is a pointer type; extract its pointee.
                let final_pointee = if let Type::Pointer {
                    pointee_type_id: ptid,
                    ..
                } = &self.module.types[&result_type_id]
                {
                    *ptid
                } else {
                    current_type_id
                };
                Ok(Pointer::StorageBuffer {
                    binding: *binding,
                    byte_offset: offset,
                    pointee_type_id: final_pointee,
                })
            }
            _ => Err(Error::new("AccessChain on non-StorageBuffer pointer")),
        }
    }

    fn get_type_array_stride(&self, type_id: u32) -> Result<u32, Error> {
        if let Some(decorations) = self.module.decorations.get(&type_id) {
            for dec in decorations {
                if let Decoration::ArrayStride(stride) = dec {
                    return Ok(*stride);
                }
            }
        }
        // For arrays of wrapper structs, check if the element type has a stride.
        match &self.module.types[&type_id] {
            Type::Array {
                element_type_id, ..
            }
            | Type::RuntimeArray { element_type_id } => {
                let element_size = self.compute_type_size(*element_type_id)?;
                Ok(element_size)
            }
            _ => Err(Error::new(format!("no ArrayStride for type %{type_id}"))),
        }
    }

    fn compute_type_size(&self, type_id: u32) -> Result<u32, Error> {
        match &self.module.types[&type_id] {
            Type::Int { width, .. } | Type::Float { width } => Ok(*width / 8),
            Type::Bool => Ok(4),
            Type::Vector {
                component_type_id,
                count,
            } => {
                let component_size = self.compute_type_size(*component_type_id)?;
                Ok(component_size * *count)
            }
            _ => Err(Error::new(format!(
                "cannot compute size for type %{type_id}"
            ))),
        }
    }

    fn get_member_offset(&self, struct_type_id: u32, member_index: u32) -> Result<u32, Error> {
        if let Some(decorations) = self
            .module
            .member_decorations
            .get(&(struct_type_id, member_index))
        {
            for dec in decorations {
                if let Decoration::Offset(offset) = dec {
                    return Ok(*offset);
                }
            }
        }
        Err(Error::new(format!(
            "no Offset decoration for member {member_index} of type %{struct_type_id}"
        )))
    }

    fn get_buffer(&self, binding: u32) -> Result<&[u32], Error> {
        match binding {
            2 => Ok(self.display_buffer),
            3 => Ok(self.font_rom_buffer),
            4 => Ok(self.pegc_buffer),
            _ => Err(Error::new(format!("unknown buffer binding: {binding}"))),
        }
    }

    fn bitcast(&self, result_type_id: u32, operand: &Value) -> Result<Value, Error> {
        let bits = match operand {
            Value::U32(v) => *v,
            Value::F32(v) => v.to_bits(),
            _ => {
                return Err(Error::new(format!(
                    "bitcast from unsupported value: {operand:?}"
                )));
            }
        };
        match &self.module.types[&result_type_id] {
            Type::Int { .. } => Ok(Value::U32(bits)),
            Type::Float { .. } => Ok(Value::F32(f32::from_bits(bits))),
            _ => Err(Error::new(format!(
                "bitcast to unsupported type %{result_type_id}"
            ))),
        }
    }

    fn execute_glsl_ext(
        &self,
        instruction: GlslExtInst,
        operand_ids: &[u32],
    ) -> Result<Value, Error> {
        match instruction {
            GlslExtInst::Pow => {
                let base = self.get_value(operand_ids[0])?.as_f32();
                let exponent = self.get_value(operand_ids[1])?.as_f32();
                Ok(Value::F32(base.powf(exponent)))
            }
            GlslExtInst::UMin => {
                let a = self.get_value(operand_ids[0])?.as_u32();
                let b = self.get_value(operand_ids[1])?.as_u32();
                Ok(Value::U32(a.min(b)))
            }
            GlslExtInst::UMax => {
                let a = self.get_value(operand_ids[0])?.as_u32();
                let b = self.get_value(operand_ids[1])?.as_u32();
                Ok(Value::U32(a.max(b)))
            }
            GlslExtInst::SMax => {
                let a = self.get_value(operand_ids[0])?.as_u32() as i32;
                let b = self.get_value(operand_ids[1])?.as_u32() as i32;
                Ok(Value::U32(a.max(b) as u32))
            }
        }
    }
}
