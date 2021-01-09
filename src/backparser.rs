use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use super::parser::*;
pub use super::parser::{Proof, Step, ElabStep};

const BUFFER_SIZE: usize = 0x4000;

pub struct VecBackParser(pub Vec<u8>);

impl Iterator for VecBackParser {
  type Item = Segment;

  fn next(&mut self) -> Option<Segment> {
    let (&n, most) = self.0.split_last()?;
    if n != 0 { panic!("expected 0 byte") }
    let i = most.iter().rposition(|&n| n == 0).map_or(0, |i| i + 1);
    Some(Bin::segment(&mut self.0.drain(i..)))
  }
}

pub struct BackParser<M> {
  file: File,
  remaining: usize,
  pos: usize,
  last_read: usize,
  buffers: Vec<Box<[u8]>>,
  free: Vec<Box<[u8]>>,
  _mode: M,
}

impl<M> BackParser<M> {
  pub fn new(mode: M, mut file: File) -> io::Result<BackParser<M>> {
    let len = file.metadata()?.len() as usize;
    let pos = len.checked_sub(1).map_or(0, |l| l % BUFFER_SIZE + 1);
    file.seek(SeekFrom::End(-(pos as i64)))?;
    let mut buf = Box::new([0; BUFFER_SIZE]);
    file.read_exact(&mut buf[..pos])?;
    Ok(BackParser {
      file,
      remaining: len / BUFFER_SIZE - if pos == BUFFER_SIZE {1} else {0},
      pos,
      last_read: pos,
      buffers: vec![buf],
      free: Vec::new(),
      _mode: mode
    })
  }

  fn read_chunk(&mut self) -> io::Result<Option<Box<[u8]>>> {
    if self.remaining == 0 { return Ok(None) }
    let mut buf: Box<[u8]> = self.free.pop().unwrap_or_else(|| Box::new([0; BUFFER_SIZE]));
    self.file.seek(SeekFrom::Current(-((BUFFER_SIZE + self.last_read) as i64)))?;
    self.file.read_exact(&mut buf)?;
    self.last_read = BUFFER_SIZE;
    self.remaining -= 1;
    Ok(Some(buf))
  }
}

impl<M: Mode> BackParser<M> {
  fn parse_segment_from(&mut self, b: usize, i: usize) -> Segment {
    if b == 0 {
      let res = M::segment(&mut self.buffers[0][i..self.pos].iter().copied());
      self.pos = i;
      return res
    } else {
      let res = M::segment(
        &mut self.buffers[b][i..].iter()
          .chain(self.buffers[1..b].iter().rev().flat_map(|buf| buf.iter()))
          .chain(self.buffers[0][..self.pos].iter()).copied());
      self.pos = i;
      self.free.extend(self.buffers.drain(0..b));
      return res
    }
  }
}

impl<M: Mode> Iterator for BackParser<M> {
  type Item = Segment;

  fn next(&mut self) -> Option<Segment> {
    for b in 0.. {
      let buf: &[u8] = match self.buffers.get(b) {
        None => match self.read_chunk().expect("could not read from proof file") {
          None => {
            if b == 1 && self.pos == 0 { break }
            return Some(self.parse_segment_from(b-1, 0))
          },
          Some(buf) => { self.buffers.push(buf); self.buffers.last().unwrap() }
        },
        Some(buf) => buf
      };
      if b == 0 {
        if self.pos != 0 {
          for i in (0..self.pos-1).rev() {
            if M::back_scan(buf[i]) { return Some(self.parse_segment_from(b, i + M::OFFSET)) }
          }
        }
      } else {
        for (i, &v) in buf.iter().enumerate().rev() {
          if M::back_scan(v) { return Some(self.parse_segment_from(b, i + M::OFFSET)) }
        }
      }
    }
    None
  }
}

pub struct StepIter<I>(pub I);

impl<I: Iterator<Item=Segment>> Iterator for StepIter<I> {
  type Item = Step;

  fn next(&mut self) -> Option<Step> {
    match self.0.next() {
      None => None,
      Some(Segment::Orig(idx, vec)) => Some(Step::Orig(idx, vec)),
      Some(Segment::Add(idx, vec)) => Some(Step::Add(idx, vec, None)),
      Some(Segment::Del(idx, vec)) => Some(Step::Del(idx, vec)),
      Some(Segment::Reloc(relocs)) => Some(Step::Reloc(relocs)),
      Some(Segment::Final(idx, vec)) => Some(Step::Final(idx, vec)),
      Some(Segment::LProof(steps)) => match self.0.next() {
        Some(Segment::Add(idx, vec)) =>
          Some(Step::Add(idx, vec, Some(Proof::LRAT(steps)))),
        _ => panic!("'l' step not preceded by 'a' step")
      },
      Some(Segment::Todo(idx)) => Some(Step::Todo(idx)),
    }
  }
}

pub struct ElabStepIter<I>(pub I);

impl<I: Iterator<Item=Segment>> Iterator for ElabStepIter<I> {
  type Item = ElabStep;

  fn next(&mut self) -> Option<ElabStep> {
    match self.0.next() {
      None => None,
      Some(Segment::Orig(idx, vec)) => Some(ElabStep::Orig(idx, vec)),
      Some(Segment::Add(_, _)) => panic!("add step has no proof"),
      Some(Segment::Del(idx, vec)) =>
        {assert!(vec.is_empty()); Some(ElabStep::Del(idx))},
      Some(Segment::Reloc(relocs)) => Some(ElabStep::Reloc(relocs)),
      Some(Segment::LProof(steps)) => match self.0.next() {
        Some(Segment::Add(idx, vec)) =>
          Some(ElabStep::Add(idx, vec, steps)),
        _ => panic!("'l' step not preceded by 'a' step")
      },
      Some(_) => self.next(),
    }
  }
}
