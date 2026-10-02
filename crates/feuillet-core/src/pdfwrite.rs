//! Sérialisation d'objets PDF (lopdf n'exporte pas son `Writer`).

use lopdf::{Dictionary, Object, ObjectId, StringFormat};
use std::io::Write;

fn is_regular(c: u8) -> bool {
    !(c.is_ascii_whitespace() || b"()<>[]{}/%#".contains(&c) || !(33..=126).contains(&c))
}

pub fn write_name(out: &mut Vec<u8>, name: &[u8]) {
    out.push(b'/');
    for &c in name {
        if is_regular(c) {
            out.push(c);
        } else {
            let _ = write!(out, "#{c:02X}");
        }
    }
}

pub fn write_string(out: &mut Vec<u8>, s: &[u8], format: StringFormat) {
    match format {
        StringFormat::Hexadecimal => {
            out.push(b'<');
            for b in s {
                let _ = write!(out, "{b:02X}");
            }
            out.push(b'>');
        }
        StringFormat::Literal => {
            out.push(b'(');
            for &b in s {
                match b {
                    b'(' | b')' | b'\\' => out.extend_from_slice(&[b'\\', b]),
                    b'\r' => out.extend_from_slice(b"\\r"),
                    b'\n' => out.extend_from_slice(b"\\n"),
                    _ => out.push(b),
                }
            }
            out.push(b')');
        }
    }
}

pub fn write_real(out: &mut Vec<u8>, v: f32) {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e9 {
        let _ = write!(out, "{}", v as i64);
    } else {
        let s = format!("{:.4}", if v.is_finite() { v } else { 0.0 });
        let s = s.trim_end_matches('0').trim_end_matches('.');
        out.extend_from_slice(if s == "-0" { b"0" } else { s.as_bytes() });
    }
}

pub fn write_dict(out: &mut Vec<u8>, d: &Dictionary) {
    out.extend_from_slice(b"<<");
    for (k, v) in d.iter() {
        write_name(out, k);
        out.push(b' ');
        write_object(out, v);
    }
    out.extend_from_slice(b">>");
}

pub fn write_object(out: &mut Vec<u8>, obj: &Object) {
    match obj {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Object::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Object::Real(r) => write_real(out, *r),
        Object::Name(n) => write_name(out, n),
        Object::String(s, f) => write_string(out, s, *f),
        Object::Array(a) => {
            out.push(b'[');
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_object(out, v);
            }
            out.push(b']');
        }
        Object::Dictionary(d) => write_dict(out, d),
        Object::Stream(s) => {
            let mut dict = s.dict.clone();
            dict.set("Length", s.content.len() as i64);
            write_dict(out, &dict);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&s.content);
            out.extend_from_slice(b"\nendstream");
        }
        Object::Reference((id, generation)) => {
            let _ = write!(out, "{id} {generation} R");
        }
    }
}

/// Écrit `n g obj … endobj` et renvoie le décalage de début.
pub fn write_indirect(out: &mut Vec<u8>, id: ObjectId, obj: &Object) -> usize {
    let offset = out.len();
    let _ = writeln!(out, "{} {} obj", id.0, id.1);
    write_object(out, obj);
    out.extend_from_slice(b"\nendobj\n");
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn serializes_objects() {
        let mut out = vec![];
        let d = dictionary! { "Type" => "Annot", "Rect" => vec![1.into(), Object::Real(2.5)], "T" => Object::string_literal("a(b)") , "N" => Object::Name(b"A B".to_vec()) };
        write_object(&mut out, &Object::Dictionary(d));
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<</Type /Annot/Rect [1 2.5]/T (a\\(b\\))/N /A#20B>>"
        );
    }
}
