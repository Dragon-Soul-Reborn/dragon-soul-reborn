//! Codec for Dragon Soul `.aup` files: a u32le uncompressed size, then an MSB-first LZSS bit stream over a
//! 16 KiB ring buffer (writes start at index 1). `1 + 8 bits` is a literal; `0 + 14 bits offset + 4 bits length`
//! copies `length + 3` bytes from that absolute ring position; offset 0 ends the stream.
//! A byte-exact port of the project's Python codec (format reverse-engineered by PredatorIWD, MIT).

use std::collections::HashMap;

const RING: usize = 0x4000;
const MASK: usize = RING - 1;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 18;
/// Keeps the match source far enough back that a copy never reads a slot it has just overwritten.
const MAX_DIST: usize = RING - MAX_MATCH;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AupError {
    #[error("too small for an .aup header")]
    TooSmall,
    #[error("size mismatch: header {header}, decoded {decoded}")]
    SizeMismatch { header: usize, decoded: usize },
}

pub fn decode(data: &[u8]) -> Result<Vec<u8>, AupError> {
    if data.len() < 4 {
        return Err(AupError::TooSmall);
    }
    let size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let mut p = 4;
    // Bit buffer; past the end it reads zeros, which decode as the end marker.
    let mut acc: u64 = 0;
    let mut nacc: u32 = 0;
    let mut ring = vec![0u8; RING];
    let mut ri = 1usize;
    let mut out = Vec::with_capacity(size);
    while out.len() <= size {
        while nacc < 19 {
            acc = (acc << 8) | u64::from(data.get(p).copied().unwrap_or(0));
            p += 1;
            nacc += 8;
        }
        nacc -= 1;
        if (acc >> nacc) & 1 == 1 {
            nacc -= 8;
            let b = ((acc >> nacc) & 0xFF) as u8;
            out.push(b);
            ring[ri] = b;
            ri = (ri + 1) & MASK;
        } else {
            nacc -= 14;
            let mut off = ((acc >> nacc) as usize) & MASK;
            if off == 0 {
                break;
            }
            nacc -= 4;
            let len = ((acc >> nacc) & 0xF) as usize + MIN_MATCH;
            for _ in 0..len {
                let b = ring[off];
                out.push(b);
                ring[ri] = b;
                ri = (ri + 1) & MASK;
                off = (off + 1) & MASK;
            }
        }
        acc &= (1u64 << nacc) - 1;
    }
    if out.len() != size {
        return Err(AupError::SizeMismatch {
            header: size,
            decoded: out.len(),
        });
    }
    Ok(out)
}

struct BitWriter {
    buf: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, width: u32) {
        self.acc = (self.acc << width) | u64::from(value);
        self.n += width;
        while self.n >= 8 {
            self.n -= 8;
            self.buf.push(((self.acc >> self.n) & 0xFF) as u8);
        }
        self.acc &= (1u64 << self.n) - 1;
    }

    fn flush(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.buf.push(((self.acc << (8 - self.n)) & 0xFF) as u8);
        }
        self.buf
    }
}

pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter {
        buf: Vec::with_capacity(data.len() / 2 + 16),
        acc: 0,
        n: 0,
    };
    let mut heads: HashMap<[u8; 3], Vec<usize>> = HashMap::new();
    let n = data.len();
    let mut i = 0;
    while i < n {
        let mut best_len = 0;
        let mut best_src = 0;
        if i + MIN_MATCH <= n {
            if let Some(chain) = heads.get(&[data[i], data[i + 1], data[i + 2]]) {
                let limit = MAX_MATCH.min(n - i);
                for &j in chain.iter().rev() {
                    if i - j > MAX_DIST {
                        break;
                    }
                    if (1 + j) & MASK == 0 {
                        continue; // ring offset 0 is the end marker
                    }
                    let mut k = MIN_MATCH;
                    while k < limit && data[j + k] == data[i + k] {
                        k += 1;
                    }
                    if k > best_len {
                        best_len = k;
                        best_src = j;
                        if k == limit {
                            break;
                        }
                    }
                }
            }
        }
        let step = if best_len >= MIN_MATCH { best_len } else { 1 };
        if step == 1 {
            w.put(1, 1);
            w.put(u32::from(data[i]), 8);
        } else {
            w.put(0, 1);
            w.put(((1 + best_src) & MASK) as u32, 14);
            w.put((best_len - MIN_MATCH) as u32, 4);
        }
        for p in i..(i + step).min((n + 1).saturating_sub(MIN_MATCH)) {
            let chain = heads
                .entry([data[p], data[p + 1], data[p + 2]])
                .or_default();
            chain.push(p);
            if chain.len() > 64 {
                chain.drain(..32);
            }
        }
        i += step;
    }
    w.put(0, 1);
    w.put(0, 14);
    let mut out = (n as u32).to_le_bytes().to_vec();
    out.extend(w.flush());
    out
}
