use super::domain::*;
pub(crate) trait BudgetLedger {
    fn approve(&mut self, a: &Allowance) -> Result<()>;
    fn reserve(&mut self, r: &Request) -> Result<Intent>;
    fn claim(&mut self, id: &str) -> Result<bool>;
    fn unknown(&mut self, id: &str) -> Result<()>;
    fn cancel(&mut self, id: &str) -> Result<()>;
    fn reconcile(&mut self, id: &str, r: &Receipt) -> Result<()>;
}
pub(crate) trait BudgetQueries {
    fn list_budgets(&mut self, limit: u16) -> Result<Vec<Balance>>;
    fn list_intents(&mut self, account: Option<&str>, limit: u16) -> Result<Vec<Intent>>;
    fn balance(&mut self, id: &str) -> Result<Balance>;
    fn intent(&mut self, id: &str) -> Result<Option<Intent>>;
    fn receipts(&mut self, id: &str) -> Result<Vec<Receipt>>;
}
