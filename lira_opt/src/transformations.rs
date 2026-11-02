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

    pub fn run_ruleset_transformations_operations(
        &mut self,
        deadline: Instant,
    ) -> anyhow::Result<()> {
        self.enrich(deadline)?;
        while self.run_ruleset("transformations: operations") {
            anyhow::ensure!(Instant::now() < deadline, "timeout in ruleset operations");
            self.enrich(deadline)?;
        }
        self.enrich(deadline)?;
        Ok(())
    }
    pub fn run_ruleset_transformations_vector(&mut self, deadline: Instant) -> anyhow::Result<()> {
        self.enrich(deadline)?;
        while self.run_ruleset("transformations: vector") {
            anyhow::ensure!(Instant::now() < deadline, "timeout in ruleset vector");
            self.enrich(deadline)?;
        }
        self.enrich(deadline)?;
        Ok(())
    }
}
