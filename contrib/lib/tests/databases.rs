#[cfg(all(feature = "diesel_sqlite_pool", feature = "diesel_postgres_pool"))]
mod databases_tests {
    use rocket_contrib::databases::{database, diesel};

    #[database("foo")]
    struct TempStorage(diesel::SqliteConnection);

    #[database("bar")]
    struct PrimaryDb(diesel::PgConnection);
}

#[cfg(all(feature = "databases", feature = "sqlite_pool"))]
#[cfg(test)]
mod rusqlite_integration_test {
    use rocket_contrib::database;
    use rocket_contrib::databases::rusqlite;

    use rusqlite::types::ToSql;

    #[database("test_db")]
    struct SqliteDb(pub rusqlite::Connection);

    // Test to ensure that multiple databases of the same type can be used
    #[database("test_db_2")]
    struct SqliteDb2(pub rusqlite::Connection);

    #[rocket::async_test]
    async fn test_db() {
        use rocket::figment::{Figment, util::map};

        let options = map!["url" => ":memory:"];
        let config = Figment::from(rocket::Config::default())
            .merge(("databases", map!["test_db" => &options]))
            .merge(("databases", map!["test_db_2" => &options]));

        let rocket = rocket::custom(config)
            .attach(SqliteDb::fairing())
            .attach(SqliteDb2::fairing());

        let conn = SqliteDb::get_one(&rocket).await
            .expect("unable to get connection");

        // Rusqlite's `transaction()` method takes `&mut self`; this tests that
        // the &mut method can be called inside the closure passed to `run()`.
        conn.run(|conn| {
            let tx = conn.transaction().unwrap();
            let _: i32 = tx.query_row(
                "SELECT 1", &[] as &[&dyn ToSql], |row| row.get(0)
            ).expect("get row");

            tx.commit().expect("committed transaction");
        }).await;
    }
}

#[cfg(feature = "databases")]
#[cfg(test)]
mod drop_runtime_test {
    use r2d2::{ManageConnection, Pool};
    use rocket_contrib::databases::{database, Poolable, PoolResult};
    use tokio::runtime::Runtime;

    struct ContainsRuntime(Runtime);
    struct TestConnection;

    impl ManageConnection for ContainsRuntime {
        type Connection = TestConnection;
        type Error = std::convert::Infallible;

        fn connect(&self) -> Result<Self::Connection, Self::Error> {
            Ok(TestConnection)
        }

        fn is_valid(&self, _conn: &mut Self::Connection) -> Result<(), Self::Error> {
            Ok(())
        }

        fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
            false
        }
    }

    impl Poolable for TestConnection {
        type Manager = ContainsRuntime;
        type Error = ();

        fn pool(_db_name: &str, _rocket: &rocket::Rocket) -> PoolResult<Self> {
            let manager = ContainsRuntime(tokio::runtime::Runtime::new().unwrap());
            Ok(Pool::builder().build(manager)?)
        }
    }

    #[database("test_db")]
    struct TestDb(TestConnection);

    #[rocket::async_test]
    async fn test_drop_runtime() {
        use rocket::figment::{Figment, util::map};

        let config = Figment::from(rocket::Config::default())
            .merge(("databases", map!["test_db" => map!["url" => ""]]));

        let rocket = rocket::custom(config).attach(TestDb::fairing());
        drop(rocket);
    }
}
