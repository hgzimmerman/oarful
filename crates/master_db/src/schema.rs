diesel::table! {
    tenant (id) {
        id -> Integer,
        name -> Text,
        slug -> Text,
        db_path -> Text,
        created_at -> Timestamp,
    }
}

diesel::table! {
    superuser (id) {
        id -> Integer,
        email -> Text,
        password_hash -> Text,
        created_at -> Timestamp,
    }
}
