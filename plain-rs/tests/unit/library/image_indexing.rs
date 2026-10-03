use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
#[path = "fixtures.rs"]
mod fixtures;
use fixtures::test_db;
struct Mock {
    total: usize,
    position: usize,
    fail_at: Option<usize>,
    invalid_reply: bool,
    verified: bool,
    empty_page: bool,
}
impl Mock {
    fn new(total: usize) -> Self {
        Self {
            total,
            position: 0,
            fail_at: None,
            invalid_reply: false,
            verified: false,
            empty_page: false,
        }
    }
}
fn input(id: &str) -> EmbeddingInput {
    EmbeddingInput {
        id: id.into(),
        path: format!("/image/{id}"),
        embedding_base64: STANDARD.encode(1.0_f32.to_be_bytes()),
    }
}
impl Provider for Mock {
    fn snapshot(&mut self) -> LibraryResult<Snapshot> {
        Ok(Snapshot {
            revision: "one".into(),
            total: self.total,
        })
    }
    fn page(&mut self, _: &str, _: &str, limit: usize) -> LibraryResult<Page> {
        if self.fail_at.is_some_and(|at| self.position >= at) {
            return Err(invalid("permission revoked"));
        }
        if self.empty_page {
            return Ok(Page {
                revision: "one".into(),
                items: Vec::new(),
                next_cursor: "next".into(),
                done: false,
            });
        }
        let end = (self.position + limit).min(self.total);
        let items = (self.position..end)
            .map(|id| Image {
                id: id.to_string(),
                path: format!("/image/{id}"),
            })
            .collect();
        self.position = end;
        Ok(Page {
            revision: "one".into(),
            items,
            next_cursor: end.to_string(),
            done: end == self.total,
        })
    }
    fn embed(&mut self, _: &str, items: &[Image]) -> LibraryResult<Embedded> {
        Ok(Embedded {
            items: if self.invalid_reply {
                Vec::new()
            } else {
                items.iter().map(|i| input(&i.id)).collect()
            },
            skipped_ids: Vec::new(),
        })
    }
    fn resolve(&mut self, _: &str, ids: &[String]) -> LibraryResult<Vec<Image>> {
        Ok(ids
            .iter()
            .filter(|id| id.parse::<usize>().is_ok_and(|value| value < self.total))
            .map(|id| Image {
                id: id.clone(),
                path: format!("/image/{id}"),
            })
            .collect())
    }
    fn verify(&mut self, _: &str) -> LibraryResult<()> {
        self.verified = true;
        Ok(())
    }
}
#[derive(Default)]
struct Guard {
    cancelled: AtomicBool,
    progress: Mutex<Vec<Progress>>,
    cancel_after_save: bool,
}
impl Control for Guard {
    fn check(&self) -> LibraryResult<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(invalid("cancelled"))
        } else {
            Ok(())
        }
    }
    fn save(&self, db: &Db, items: &[EmbeddingInput]) -> LibraryResult<()> {
        self.check()?;
        image_embeddings::save(db, items)?;
        if self.cancel_after_save {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    fn remove(&self, db: &Db, ids: &[String]) -> LibraryResult<()> {
        self.check()?;
        image_embeddings::delete(db, ids)?;
        Ok(())
    }
    fn progress(&self, p: Progress) {
        self.progress.lock().unwrap().push(p);
    }
}
#[test]
fn scan_pages_large_catalog_and_removes_stale_only_after_verification() {
    let db = test_db("image_scan_pages");
    image_embeddings::save(&db, &[input("stale"), input("1")]).unwrap();
    let mut provider = Mock::new(300);
    let control = Guard::default();
    let result = scan(&db, &mut provider, &control, false).unwrap();
    assert!(provider.verified);
    assert_eq!(result.indexed, 300);
    assert_eq!(image_embeddings::count(&db).unwrap(), 300);
    assert!(
        !image_embeddings::ids(&db)
            .unwrap()
            .contains(&"stale".into())
    );
}
#[test]
fn revoked_permission_keeps_preexisting_records() {
    let db = test_db("image_scan_permission");
    image_embeddings::save(&db, &[input("stale")]).unwrap();
    let mut provider = Mock::new(300);
    provider.fail_at = Some(128);
    assert!(scan(&db, &mut provider, &Guard::default(), false).is_err());
    assert!(!provider.verified);
    assert!(
        image_embeddings::ids(&db)
            .unwrap()
            .contains(&"stale".into())
    );
}
#[test]
fn cancellation_and_incomplete_host_reply_cannot_delete_stale_records() {
    let db = test_db("image_scan_cancel");
    image_embeddings::save(&db, &[input("stale")]).unwrap();
    let mut provider = Mock::new(50);
    let control = Guard {
        cancel_after_save: true,
        ..Default::default()
    };
    assert!(scan(&db, &mut provider, &control, false).is_err());
    assert!(!provider.verified);
    assert!(
        image_embeddings::ids(&db)
            .unwrap()
            .contains(&"stale".into())
    );
    let mut provider = Mock::new(50);
    provider.invalid_reply = true;
    assert!(scan(&db, &mut provider, &Guard::default(), false).is_err());
    assert!(!provider.verified);
}

#[test]
fn empty_nonterminal_catalog_page_cannot_loop_or_purge_cache() {
    let db = test_db("image_scan_empty_page");
    image_embeddings::save(&db, &[input("stale")]).unwrap();
    let mut provider = Mock::new(1);
    provider.empty_page = true;
    assert!(scan(&db, &mut provider, &Guard::default(), false).is_err());
    assert!(!provider.verified);
    assert_eq!(image_embeddings::ids(&db).unwrap(), ["stale"]);
}

#[test]
fn selected_index_deduplicates_and_keeps_unrelated_cache() {
    let db = test_db("image_selected");
    image_embeddings::save(&db, &[input("outside"), input("missing")]).unwrap();
    let mut provider = Mock::new(3);
    selected(
        &db,
        &mut provider,
        &Guard::default(),
        &["1".into(), "2".into(), "1".into(), "missing".into()],
    )
    .unwrap();
    let ids = image_embeddings::ids(&db).unwrap();
    assert!(provider.verified);
    assert!(ids.contains(&"outside".into()));
    assert!(ids.contains(&"1".into()));
    assert!(ids.contains(&"2".into()));
    assert!(!ids.contains(&"missing".into()));
}
