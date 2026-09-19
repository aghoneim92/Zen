use crate::*;
use std::cmp::Ordering;

pub(crate) fn literal(literal: &Literal, ty: &Type) -> Result<Value, &'static str> {
    Ok(match literal {
        Literal::Unit => Value::Unit,
        Literal::Bool(v) => Value::Bool(*v),
        Literal::String(v) => Value::String(v.as_str().into()),
        Literal::Char(v) => Value::Char(*v),
        Literal::Float(v) => Value::Float(*v),
        Literal::Float32(v) => Value::Float32(*v),
        Literal::Integer {
            negative,
            magnitude,
        } => {
            let mut value = BigInt::parse_bytes(magnitude.as_bytes(), 10)
                .ok_or("invalid HIR integer magnitude")?;
            if *negative {
                value = -value;
            }
            if *ty == Type::Int {
                Value::Int(value)
            } else if matches!(
                ty,
                Type::I8
                    | Type::I16
                    | Type::I32
                    | Type::I64
                    | Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::U64
            ) {
                Value::FixedInteger {
                    ty: ty.clone(),
                    value,
                }
            } else {
                return Err("integer literal has noninteger HIR type");
            }
        }
    })
}
pub(crate) fn unary(op: UnaryOp, ty: &Type, v: Value) -> Result<Value, &'static str> {
    Ok(match (op, ty, v) {
        (UnaryOp::Not, Type::Bool, Value::Bool(v)) => Value::Bool(!v),
        (UnaryOp::Negate, Type::Int, Value::Int(v)) => Value::Int(-v),
        (UnaryOp::Negate, Type::Float, Value::Float(v)) => Value::Float(-v),
        (UnaryOp::Negate, Type::F32, Value::Float32(v)) => Value::Float32(-v),
        _ => return Err("unary operand disagrees with checked operation"),
    })
}
pub(crate) fn binary(op: BinaryOp, ty: &Type, a: Value, b: Value) -> Result<Value, &'static str> {
    use BinaryOp::*;
    if matches!(op, Equal | NotEqual) {
        let equal = a.equals(&b)?;
        return Ok(Value::Bool(if op == Equal { equal } else { !equal }));
    }
    if matches!(op, Less | LessEqual | Greater | GreaterEqual) {
        let order = match (ty, &a, &b) {
            (Type::Int, Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Type::Float, Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Type::F32, Value::Float32(a), Value::Float32(b)) => a.partial_cmp(b),
            (
                t,
                Value::FixedInteger { ty: x, value: a },
                Value::FixedInteger { ty: y, value: b },
            ) if t == x && t == y => Some(a.cmp(b)),
            _ => return Err("ordering operands disagree with checked operation"),
        };
        return Ok(Value::Bool(match op {
            Less => order == Some(Ordering::Less),
            LessEqual => matches!(order, Some(Ordering::Less | Ordering::Equal)),
            Greater => order == Some(Ordering::Greater),
            GreaterEqual => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
            _ => unreachable!(),
        }));
    }
    Ok(match (ty, a, b) {
        (Type::Int, Value::Int(a), Value::Int(b)) => Value::Int(match op {
            Add => a + b,
            Subtract => a - b,
            Multiply => a * b,
            _ => unreachable!(),
        }),
        (Type::Float, Value::Float(a), Value::Float(b)) => Value::Float(match op {
            Add => a + b,
            Subtract => a - b,
            Multiply => a * b,
            _ => unreachable!(),
        }),
        (Type::F32, Value::Float32(a), Value::Float32(b)) => Value::Float32(match op {
            Add => a + b,
            Subtract => a - b,
            Multiply => a * b,
            _ => unreachable!(),
        }),
        (Type::String, Value::String(a), Value::String(b)) if op == Add => {
            Value::String(format!("{a}{b}").into())
        }
        _ => return Err("arithmetic operands disagree with checked operation"),
    })
}

impl Machine<'_> {
    pub(crate) fn intrinsic(
        &self,
        intrinsic: Intrinsic,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value, InterpreterError> {
        let bad = || self.invariant("invalid collection intrinsic arguments", span);
        let option = |value: Option<Value>| Value::Enum {
            variant: VariantId {
                owner: zen_hir::OPTION,
                index: if value.is_some() { 0 } else { 1 },
            },
            payload: value.into_iter().collect(),
        };
        match (intrinsic, args.as_slice()) {
            (Intrinsic::ListFirst, [Value::List(items)]) => Ok(option(items.first().cloned())),
            (Intrinsic::ListGet, [Value::List(items), Value::Int(index)]) => {
                // Negative and arbitrarily large indices are normal absence.
                let index = usize::try_from(index).ok();
                Ok(option(index.and_then(|i| items.get(i)).cloned()))
            }
            (Intrinsic::ListAppend, [Value::List(items), value]) => {
                let mut items = items.as_ref().clone();
                items.push(value.clone());
                Ok(Value::List(items.into()))
            }
            (Intrinsic::MapGet, [Value::Map(items), key]) => {
                for (k, v) in items.iter() {
                    if k.equals(key).map_err(|m| self.invariant(m, span))? {
                        return Ok(option(Some(v.clone())));
                    }
                }
                Ok(option(None))
            }
            _ => Err(bad()),
        }
    }
}
