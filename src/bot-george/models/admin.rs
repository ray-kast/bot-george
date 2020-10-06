#[derive(Queryable)]
pub struct Post {
    pub user_id: u64,
    pub role: String,
}
