pub use num_bigint::BigInt;
use std::{collections::BTreeMap, fmt, sync::Arc};
use zen_hir::{CallTarget, ExprId, ParamId, SymbolId, Type, TypeId, VariantId};

/// Owned Zen values. Sharing is immutable; rebinding never changes an old value.
#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Task(crate::TaskId),
    Bool(bool),
    Int(BigInt),
    FixedInteger {
        ty: Type,
        value: BigInt,
    },
    Float(f64),
    Float32(f32),
    String(Arc<str>),
    Char(char),
    List(Arc<Vec<Value>>),
    /// Host-supplied collections only: source has no Set/Map constructors yet.
    Set(Arc<Vec<Value>>),
    Map(Arc<Vec<(Value, Value)>>),
    Struct {
        nominal: TypeId,
        fields: Vec<Value>,
    },
    Enum {
        variant: VariantId,
        payload: Vec<Value>,
    },
    Callable(Arc<CallableValue>),
}

#[derive(Clone, Debug)]
pub enum CallableValue {
    Resolved {
        target: CallTarget,
        receiver: Option<Value>,
        types: BTreeMap<ParamId, Type>,
    },
    Closure {
        expression: ExprId,
        captures: BTreeMap<SymbolId, Value>,
        types: BTreeMap<ParamId, Type>,
    },
}

impl Value {
    /// Language equality. An error means invalid HIR/host data, not a Zen failure.
    pub fn equals(&self, other: &Self) -> Result<bool, &'static str> {
        Ok(match (self, other) {
            (Self::Unit, Self::Unit) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::FixedInteger { ty: a, value: x }, Self::FixedInteger { ty: b, value: y })
                if a == b =>
            {
                x == y
            }
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Float32(a), Self::Float32(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Char(a), Self::Char(b)) => a == b,
            (
                Self::Struct {
                    nominal: a,
                    fields: x,
                },
                Self::Struct {
                    nominal: b,
                    fields: y,
                },
            ) if a == b => equal_fields(x, y)?,
            (
                Self::Enum {
                    variant: a,
                    payload: x,
                },
                Self::Enum {
                    variant: b,
                    payload: y,
                },
            ) if a.owner == b.owner => a == b && equal_fields(x, y)?,
            _ => return Err("equality on incompatible or non-equality-capable values"),
        })
    }
}
fn equal_fields(a: &[Value], b: &[Value]) -> Result<bool, &'static str> {
    if a.len() != b.len() {
        return Err("aggregate payload length mismatch");
    }
    for (x, y) in a.iter().zip(b) {
        if !x.equals(y)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Deterministic developer rendering, not a Zen serialization format.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task(id) => write!(f, "Task#{}", id.index()),
            Self::Unit => write!(f, "Unit"),
            Self::Bool(x) => write!(f, "Bool({x})"),
            Self::Int(x) => write!(f, "Int({x})"),
            Self::FixedInteger { ty, value } => write!(f, "{ty:?}({value})"),
            Self::Float(x) => write!(f, "Float({x:?})"),
            Self::Float32(x) => write!(f, "F32({x:?})"),
            Self::String(x) => write!(f, "String({x:?})"),
            Self::Char(x) => write!(f, "Char({x:?})"),
            Self::List(x) | Self::Set(x) => {
                write!(
                    f,
                    "{}[",
                    if matches!(self, Self::List(_)) {
                        "List"
                    } else {
                        "Set"
                    }
                )?;
                values(f, x)?;
                write!(f, "]")
            }
            Self::Struct { nominal, fields } => {
                write!(f, "Struct#{}(", nominal.0)?;
                values(f, fields)?;
                write!(f, ")")
            }
            Self::Enum { variant, payload } => {
                write!(f, "Enum#{}.{}(", variant.owner.0, variant.index)?;
                values(f, payload)?;
                write!(f, ")")
            }
            Self::Map(entries) => {
                write!(f, "Map[")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "]")
            }
            Self::Callable(_) => write!(f, "<function>"),
        }
    }
}
fn values(f: &mut fmt::Formatter<'_>, values: &[Value]) -> fmt::Result {
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{value}")?;
    }
    Ok(())
}
