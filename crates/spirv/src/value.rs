#[derive(Debug, Clone)]
pub(crate) enum Value {
    Bool(bool),
    U32(u32),
    F32(f32),
    Vector(Vec<Value>),
    Pointer(Pointer),
}

#[derive(Debug, Clone)]
pub(crate) enum Pointer {
    StorageBuffer {
        binding: u32,
        byte_offset: u32,
        pointee_type_id: u32,
    },
    Output {
        variable_id: u32,
    },
    Input {
        variable_id: u32,
    },
}

impl Value {
    pub(crate) fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => panic!("expected Bool, got {self:?}"),
        }
    }

    pub(crate) fn as_u32(&self) -> u32 {
        match self {
            Value::U32(v) => *v,
            _ => panic!("expected U32, got {self:?}"),
        }
    }

    pub(crate) fn as_f32(&self) -> f32 {
        match self {
            Value::F32(v) => *v,
            _ => panic!("expected F32, got {self:?}"),
        }
    }

    pub(crate) fn as_vector(&self) -> &[Value] {
        match self {
            Value::Vector(v) => v,
            _ => panic!("expected Vector, got {self:?}"),
        }
    }

    pub(crate) fn as_pointer(&self) -> &Pointer {
        match self {
            Value::Pointer(p) => p,
            _ => panic!("expected Pointer, got {self:?}"),
        }
    }
}
