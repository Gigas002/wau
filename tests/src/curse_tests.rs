use libwau::providers::curse::Curse;
use uuid::Uuid;

#[test]
fn test_new() {
    let token = Uuid::new_v4().to_string();
    let curse_client = Curse::new(&token);
}
