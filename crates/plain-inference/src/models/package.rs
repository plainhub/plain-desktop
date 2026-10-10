use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Copy)]
enum Message {
    Model,
    Graph,
    Node,
    Attribute,
    Tensor,
    Sparse,
    Training,
    Function,
}
fn varint(file: &mut File, end: u64) -> Result<u64, String> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        if file.stream_position().map_err(|e| e.to_string())? >= end {
            return Err("Truncated ONNX protobuf".into());
        }
        let mut byte = [0];
        file.read_exact(&mut byte).map_err(|e| e.to_string())?;
        if shift == 63 && byte[0] > 1 {
            return Err("ONNX varint overflow".into());
        }
        value |= ((byte[0] & 127) as u64) << shift;
        if byte[0] < 128 {
            return Ok(value);
        }
    }
    Err("Invalid ONNX varint".into())
}
fn child(message: Message, field: u64) -> Option<Message> {
    use Message::*;
    match (message, field) {
        (Model, 7) | (Training, 1 | 2) | (Attribute, 6 | 11) => Some(Graph),
        (Model, 20) => Some(Training),
        (Model, 25) => Some(Function),
        (Graph, 1) | (Function, 7) => Some(Node),
        (Node, 5) | (Function, 11) => Some(Attribute),
        (Graph, 5) | (Attribute, 5 | 10) | (Sparse, 1 | 2) => Some(Tensor),
        (Graph, 15) | (Attribute, 22 | 23) => Some(Sparse),
        _ => None,
    }
}
fn scan(file: &mut File, message: Message, end: u64, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("ONNX nesting exceeds limit".into());
    }
    while file.stream_position().map_err(|e| e.to_string())? < end {
        let key = varint(file, end)?;
        let field = key >> 3;
        if field == 0 {
            return Err("Invalid ONNX protobuf field".into());
        }
        if matches!(message, Message::Tensor) && field == 13 {
            return Err("External ONNX weights are unsupported; export embedded weights".into());
        }
        let count = match key & 7 {
            0 => {
                let value = varint(file, end)?;
                if matches!(message, Message::Tensor) && field == 14 && value != 0 {
                    return Err("External ONNX weights are unsupported".into());
                }
                continue;
            }
            1 => 8,
            2 => varint(file, end)?,
            5 => 4,
            _ => return Err("Unsupported ONNX protobuf wire type".into()),
        };
        let position = file.stream_position().map_err(|e| e.to_string())?;
        let next = position
            .checked_add(count)
            .filter(|value| *value <= end)
            .ok_or("Truncated ONNX protobuf payload")?;
        if key & 7 == 2 {
            if let Some(kind) = child(message, field) {
                scan(file, kind, next, depth + 1)?;
            }
        }
        file.seek(SeekFrom::Start(next))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn validate(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let end = file.metadata().map_err(|e| e.to_string())?.len();
    scan(&mut file, Message::Model, end, 0)
}
