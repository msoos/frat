pub mod dimacs;
mod parser;
pub mod backparser;
mod lratchk;
mod serialize;

use std::env;
use std::io;
use std::fs::File;

fn main() -> io::Result<()> {
  let mut args = env::args().skip(1);
  let proof = File::open(args.next().expect("missing proof file"))?;
	lratchk::check_proof(proof)
}
