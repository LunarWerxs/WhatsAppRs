(function () {
  var log = function (m) { window.__wa_errors.push('PERF ' + m); };
  var N = 3000;
  var req = indexedDB.open('perfTest', 1);
  req.onupgradeneeded = function (e) {
    var db = e.target.result;
    if (db.objectStoreNames.contains('msgs')) { db.deleteObjectStore('msgs'); }
    var store = db.createObjectStore('msgs', { keyPath: 'id' });
    store.createIndex('byChat', 'chat', { unique: false });
  };
  req.onerror = function () { log('open failed'); };
  req.onsuccess = function (e) {
    var db = e.target.result;
    var tx = db.transaction('msgs', 'readwrite');
    var store = tx.objectStore('msgs');
    // A rough stand-in for a message: a bit of text plus some metadata.
    for (var i = 0; i < N; i++) {
      store.put({
        id: i,
        chat: 'chat' + (i % 50),
        body: 'message body number ' + i + ' with some padding to look like real text',
        ts: 1788800000 + i,
        from: 'user' + (i % 200)
      });
    }
    tx.onerror = function () { log('seed failed'); };
    tx.oncomplete = function () {
      log('seeded ' + N + ' records');
      var t0 = performance.now();
      // One index lookup that should touch 60 of 3000 records.
      var byChat = db.transaction('msgs', 'readonly').objectStore('msgs').index('byChat');
      var q = byChat.getAll('chat7');
      q.onsuccess = function (ev) {
        var idxMs = performance.now() - t0;
        log('index getAll(chat7) -> ' + (ev.target.result || []).length + ' rows in ' + idxMs.toFixed(0) + ' ms');

        // The same shape of query straight off the store's own key, for scale.
        var t1 = performance.now();
        var direct = db.transaction('msgs', 'readonly').objectStore('msgs').get(7);
        direct.onsuccess = function () {
          log('store get(7) in ' + (performance.now() - t1).toFixed(0) + ' ms');

          // Ten index lookups back to back, which is closer to what a chat app does.
          var t2 = performance.now();
          var done = 0;
          for (var k = 0; k < 10; k++) {
            var r = db.transaction('msgs', 'readonly').objectStore('msgs').index('byChat').getAllKeys('chat' + k);
            r.onsuccess = function () {
              done++;
              if (done === 10) {
                log('10 index getAllKeys in ' + (performance.now() - t2).toFixed(0) + ' ms');
              }
            };
          }
        };
      };
      q.onerror = function () { log('index getAll errored'); };
    };
  };
  return 'perf test started';
})()
