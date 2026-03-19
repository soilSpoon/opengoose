pub struct GitSource {
    pub owner_repo: String,
    pub clone_url: String,
}

pub fn parse_source(_input: &str) -> anyhow::Result<GitSource> {
    todo!()
}
