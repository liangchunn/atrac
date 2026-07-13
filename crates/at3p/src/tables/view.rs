pub fn read_u16_le(bytes: &[u8], index: usize) -> Option<u16> {
    let start = index.checked_mul(2)?;
    let raw: [u8; 2] = bytes.get(start..start + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(raw))
}

pub fn read_i16_le(bytes: &[u8], index: usize) -> Option<i16> {
    let start = index.checked_mul(2)?;
    let raw: [u8; 2] = bytes.get(start..start + 2)?.try_into().ok()?;
    Some(i16::from_le_bytes(raw))
}

pub fn read_u32_le(bytes: &[u8], index: usize) -> Option<u32> {
    let start = index.checked_mul(4)?;
    let raw: [u8; 4] = bytes.get(start..start + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(raw))
}

pub fn read_i32_le(bytes: &[u8], index: usize) -> Option<i32> {
    let start = index.checked_mul(4)?;
    let raw: [u8; 4] = bytes.get(start..start + 4)?.try_into().ok()?;
    Some(i32::from_le_bytes(raw))
}

pub fn read_f32_le(bytes: &[u8], index: usize) -> Option<f32> {
    Some(f32::from_bits(read_u32_le(bytes, index)?))
}

pub fn f32_table<const N: usize>(bytes: &[u8]) -> Option<[f32; N]> {
    if bytes.len() != N * 4 {
        return None;
    }
    Some(std::array::from_fn(|index| {
        read_f32_le(bytes, index).expect("length already checked")
    }))
}

pub fn u32_table<const N: usize>(bytes: &[u8]) -> Option<[u32; N]> {
    if bytes.len() != N * 4 {
        return None;
    }
    Some(std::array::from_fn(|index| {
        read_u32_le(bytes, index).expect("length already checked")
    }))
}

pub fn i32_table<const N: usize>(bytes: &[u8]) -> Option<[i32; N]> {
    if bytes.len() != N * 4 {
        return None;
    }
    Some(std::array::from_fn(|index| {
        read_i32_le(bytes, index).expect("length already checked")
    }))
}

pub fn u16_table<const N: usize>(bytes: &[u8]) -> Option<[u16; N]> {
    if bytes.len() != N * 2 {
        return None;
    }
    Some(std::array::from_fn(|index| {
        read_u16_le(bytes, index).expect("length already checked")
    }))
}

pub fn i16_table<const N: usize>(bytes: &[u8]) -> Option<[i16; N]> {
    if bytes.len() != N * 2 {
        return None;
    }
    Some(std::array::from_fn(|index| {
        read_i16_le(bytes, index).expect("length already checked")
    }))
}
