#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitWriterError {
    InvalidBitCount {
        nbits: u8,
    },
    InvalidTailTarget {
        bit_pos: usize,
        target_bits: usize,
    },
    OutOfSpace {
        bit_pos: usize,
        nbits: u8,
        capacity_bits: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TailWriteSummary {
    pub marker_start_bit: usize,
    pub aligned_bit_pos: usize,
    pub stuffing_bytes: usize,
}

pub struct BitWriter<'a> {
    buffer: &'a mut [u8],
    bit_pos: usize,
}

impl<'a> BitWriter<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, bit_pos: 0 }
    }

    pub fn from_bit_pos(buffer: &'a mut [u8], bit_pos: usize) -> Result<Self, BitWriterError> {
        let capacity_bits = buffer.len() * 8;
        if bit_pos > capacity_bits {
            return Err(BitWriterError::OutOfSpace {
                bit_pos,
                nbits: 0,
                capacity_bits,
            });
        }
        Ok(Self { buffer, bit_pos })
    }

    pub fn bit_pos(&self) -> usize {
        self.bit_pos
    }

    pub fn capacity_bits(&self) -> usize {
        self.buffer.len() * 8
    }

    pub fn write_bits(&mut self, value: u32, nbits: u8) -> Result<(), BitWriterError> {
        if nbits > 32 {
            return Err(BitWriterError::InvalidBitCount { nbits });
        }

        let capacity_bits = self.capacity_bits();
        let end_pos = self.bit_pos.checked_add(nbits as usize).ok_or({
            BitWriterError::OutOfSpace {
                bit_pos: self.bit_pos,
                nbits,
                capacity_bits,
            }
        })?;
        if end_pos > capacity_bits {
            return Err(BitWriterError::OutOfSpace {
                bit_pos: self.bit_pos,
                nbits,
                capacity_bits,
            });
        }

        let mut remaining = nbits as usize;
        while remaining != 0 {
            let byte_index = self.bit_pos >> 3;
            let bit_offset = (self.bit_pos & 7) as u8;
            let available = 8 - bit_offset as usize;

            if available <= remaining {
                remaining -= available;
                self.buffer[byte_index] |= ((value >> remaining) as u8) & (0xff_u8 >> bit_offset);
                self.bit_pos += available;
            } else {
                let shift = available - remaining;
                self.buffer[byte_index] |=
                    ((value as u8) << shift) & (0xff_u8 >> bit_offset) & (0xff_u8 << shift);
                self.bit_pos += remaining;
                remaining = 0;
            }
        }

        Ok(())
    }

    pub fn write_frame_tail(
        &mut self,
        target_bits: usize,
    ) -> Result<TailWriteSummary, BitWriterError> {
        let capacity_bits = self.capacity_bits();
        if target_bits > capacity_bits {
            return Err(BitWriterError::OutOfSpace {
                bit_pos: self.bit_pos,
                nbits: 0,
                capacity_bits,
            });
        }

        let marker_start_bit = self.bit_pos;
        let marker_end_bit =
            marker_start_bit
                .checked_add(2)
                .ok_or(BitWriterError::InvalidTailTarget {
                    bit_pos: self.bit_pos,
                    target_bits,
                })?;
        let aligned_bit_pos = marker_end_bit
            .checked_add(7)
            .map(|bit_pos| bit_pos & !7)
            .ok_or(BitWriterError::InvalidTailTarget {
                bit_pos: self.bit_pos,
                target_bits,
            })?;

        if target_bits < aligned_bit_pos || (target_bits & 7) != 0 {
            return Err(BitWriterError::InvalidTailTarget {
                bit_pos: self.bit_pos,
                target_bits,
            });
        }

        self.write_bits(3, 2)?;
        self.bit_pos = aligned_bit_pos;

        let stuffing_bytes = (target_bits - aligned_bit_pos) / 8;
        for _ in 0..stuffing_bytes {
            self.write_bits(1, 8)?;
        }

        Ok(TailWriteSummary {
            marker_start_bit,
            aligned_bit_pos,
            stuffing_bytes,
        })
    }
}
