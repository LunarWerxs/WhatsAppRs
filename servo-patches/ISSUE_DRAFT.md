# DRAFT bug report for servo/servo - NOT POSTED. For Michael to read, edit and file himself, or discard.
#
# Why it is a draft and not a PR: Servo's contributor guide bans contributions containing
# LLM-generated content and names Claude explicitly. The patches were written by Claude, so
# they must not be submitted as code, and a DCO sign-off on them would be false. A bug
# report describing what was FOUND (all of it measured, all reproducible on
# web.whatsapp.com) is a different thing, but it is still text written by an AI, so the
# honest route is for a person to own it: read it, rewrite it in your words if you like, and
# post it under your own account if you think it is worth their time. The patches are public
# on the LunarWerxs/servo fork only if you choose to push them; today they are local.

Title: IndexedDB and Web Locks gaps that stop web.whatsapp.com at "Loading your chats"

Loading https://web.whatsapp.com in Servo (main as of 2026-09-06, IndexedDB and service
workers enabled by pref) gets to the QR code, logs in, and then stops at "Loading your chats"
indefinitely. Running it down turned up several separate engine gaps, each of which is
individually small and each of which was masking the next. Reporting them together because
the page is a good end-to-end test for the storage stack, and separately because they are
independent fixes.

1. IndexedDB index names are made unique across the whole database. `object_store_index`
   in `components/storage/indexeddb/engines/sqlite/create.rs` declares `name ... unique`.
   Per spec (and per the Firefox `DBSchema.cpp` the file is adapted from) an index name is
   only unique within its object store. A second object store creating an index with the same
   name as one in another store fails with `UNIQUE constraint failed: object_store_index.name`
   during `createIndex`, which aborts the upgrade transaction. WhatsApp's schema reuses index
   names across stores. Expected constraint: `UNIQUE (object_store_id, name)`.

2. A backend error during an upgrade transaction panics the script thread.
   `clear_upgrade_transaction` in `idbdatabase.rs` hits an `.expect()` when the transaction
   was aborted before being recorded on the connection (the constraint failure above triggers
   it). The constellation then reports a page crash. Treating a missing upgrade transaction as
   already cleared turns this into the page's own "database error" handling instead.

3. `IDBCursor.continue()`, `continuePrimaryKey()` and `advance()` are declared in the WebIDL
   but not implemented, so a cursor can read its first record and never move. The iteration
   algorithm itself is present in the engine; only the DOM-side plumbing is missing.

4. `IDBIndex` has no query methods at all - `get`, `getKey`, `getAll`, `getAllKeys`, `count`,
   `openCursor`, `openKeyCursor` throw "is not a function" - and an index cursor leaves its
   primary key undefined after the first step. WhatsApp's sync path calls `index.getAll` and
   this is the point it actually stops at once 1-3 are fixed.

5. Web Locks (`navigator.locks`) does not exist, and this is the one that produces the silent
   hang. WhatsApp's backend worker calls `self.navigator.locks.request(name, {steal: true},
   ...)` unguarded as the first statement of its script; the statement that registers its
   message handler is the last. The call throws synchronously, the handler is never registered,
   the worker answers nothing, and the main thread waits on it with no timeout and no error
   handler. Every other `navigator.locks` call in WhatsApp's bundles is null-guarded; the
   worker's is the only one that is not. Because user scripts are injected into documents and
   never into workers, an embedder cannot polyfill this from outside; it has to be in the
   engine, and it has to be on `WorkerNavigator`, not only `Window`.

6. Smaller, found on the way: the promise-method codegen in `script_bindings/codegen/codegen.py`
   calls the wrapped extern fn by its raw IDL name, so a promise-returning method named `match`
   generates `match::<D>(...)`, which does not parse. The extern fn itself is already emitted
   as `match_`; the wrapper should use the same escape. Hit while adding `Cache.match()`.

With all of the above addressed locally, WhatsApp Web loads its real chat list in Servo. Two
things that remain missing and are worth knowing about for this page: `IDBCursor.update` /
`delete`, and `navigator.storage.getDirectory` (OPFS), which WhatsApp probes but does not yet
require.

Happy to provide the schema shape or reproduction steps for any of these.
