diesel::table! {
    tenant (id) {
        id -> Integer,
        name -> Text,
        slug -> Text,
        db_path -> Text,
        created_at -> Timestamp,
        attributes_public -> Integer,
        force_cox_stern -> Integer,
        demo_expires_at -> Nullable<Timestamp>,
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
