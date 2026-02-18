use crate::expenses::NativeCurrency;

#[derive(sqlx::FromRow)]
pub struct ExpenseEntry {
    pub description: String,
    pub date: chrono::DateTime<chrono::Utc>,

    pub amount: NativeCurrency,
}
