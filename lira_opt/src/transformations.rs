mod operations;
mod vector;

use std::time::Instant;

use crate::Theory;

impl Theory {
    pub fn add_transformation_rulesets(&mut self, max_generation_cost: usize) {
        self.set_ruleset_active("transformations: operations");
        // Note: works on generic operations
        self.add_rules_transformations_operations(max_generation_cost);

        self.set_ruleset_active("transformations: vector");
        self.add_rules_transformations_vector();

        self.set_ruleset_active("set all transformation costs to zero");
        let sel = self.sel;
        let gc = self.generation_cost;
        // Input terms start at generation 0 so the guarded class-creating
        // rules may fire once on them.
        dateg::execute! {self; (rule (query x (sel _ _)) (set 0 (gc x))) }
    }

    pub fn run_ruleset_set_transformation_costs_to_zero(&mut self) {
        self.run_ruleset("set all transformation costs to zero");
    }

    /// One pass over the ruleset, checking the budget before each rule so a
    /// single slow rule cannot run the whole pass past the deadline.
    pub fn run_ruleset_budgeted(
        &mut self,
        ruleset: &str,
        deadline: Instant,
        max_rows: usize,
    ) -> anyhow::Result<bool> {
        let rules = self.rulesets[ruleset].clone();
        let mut changed = false;
        for rule in rules {
            anyhow::ensure!(Instant::now() < deadline, "timeout in ruleset {ruleset}");
            anyhow::ensure!(self.total_rows() <= max_rows);
            changed |= self.run_rules(&[rule]);
        }
        Ok(changed)
    }
    pub fn run_ruleset_fixpoint(
        &mut self,
        ruleset: &str,
        deadline: Instant,
        max_rows: usize,
    ) -> anyhow::Result<()> {
        self.enrich(deadline)?;
        while self.run_ruleset_budgeted(ruleset, deadline, max_rows)? {
            self.enrich(deadline)?;
        }
        self.enrich(deadline)?;
        anyhow::ensure!(self.total_rows() <= max_rows);
        Ok(())
    }

    pub fn run_ruleset_transformations_operations(
        &mut self,
        deadline: Instant,
        max_rows: usize,
    ) -> anyhow::Result<()> {
        self.run_ruleset_fixpoint("transformations: operations", deadline, max_rows)
    }
    pub fn run_ruleset_transformations_vector(
        &mut self,
        deadline: Instant,
        max_rows: usize,
    ) -> anyhow::Result<()> {
        self.run_ruleset_fixpoint("transformations: vector", deadline, max_rows)
    }
}
