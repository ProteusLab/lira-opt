mod ops_generalization;
mod theory;
mod transformations;
mod utils;

pub use lira;
pub use lira_trs;
pub use lira_trs::{dateg, dateg_extractors};
pub use theory::Lira as Theory;

pub struct Arch(pub lira::Arch);

use utils::get_;
