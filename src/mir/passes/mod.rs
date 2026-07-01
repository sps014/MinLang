//! The MIR optimization pass manager and passes.

mod const_fold;
mod dce;
mod prop;
mod rc;
mod simplify_cfg;

pub use const_fold::ConstFold;
pub use dce::Dce;
pub use prop::CopyConstProp;
pub use rc::{RcElision, RcInsertion};
pub use simplify_cfg::SimplifyCfg;

use super::MirFunction;
use crate::types::TypeInterner;

/// A single function-level MIR transformation.
pub trait MirPass {
    fn name(&self) -> &'static str;
    /// Runs the pass over one function. Returns `true` if it changed anything (drives the
    /// fixpoint loop in [`PassManager::run`]).
    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool;
}

/// Runs a configured pipeline of passes to a fixpoint over each function.
pub struct PassManager {
    passes: Vec<Box<dyn MirPass>>,
    max_iterations: usize,
}

impl PassManager {
    pub fn new() -> Self {
        PassManager {
            passes: Vec::new(),
            max_iterations: 16,
        }
    }

    /// The default optimization pipeline, ordered so cheap simplifications expose work for the
    /// later ones (prop -> fold -> simplify-cfg -> dce, then RC elision).
    pub fn default_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.add(CopyConstProp);
        pm.add(ConstFold);
        pm.add(SimplifyCfg);
        pm.add(Dce);
        pm.add(RcElision);
        pm
    }

    pub fn add(&mut self, pass: impl MirPass + 'static) {
        self.passes.push(Box::new(pass));
    }

    /// Runs every pass repeatedly until none reports a change (or the iteration cap is hit).
    pub fn run(&self, func: &mut MirFunction, interner: &TypeInterner) {
        for _ in 0..self.max_iterations {
            let mut changed = false;
            for pass in &self.passes {
                changed |= pass.run(func, interner);
            }
            if !changed {
                break;
            }
        }
    }
}

impl Default for PassManager {
    fn default() -> Self {
        PassManager::new()
    }
}
