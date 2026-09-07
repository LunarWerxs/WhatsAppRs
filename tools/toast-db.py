"""Last N toasts Windows accepted, from the Notification Center database."""
import os, shutil, sqlite3, sys, datetime
n = int(sys.argv[1]) if len(sys.argv) > 1 else 10
src = os.path.join(os.environ["LOCALAPPDATA"], r"Microsoft\Windows\Notifications\wpndatabase.db")
dst = os.path.join(os.path.dirname(__file__), "wpn-copy.db")
shutil.copy(src, dst)
for ext in ("-wal", "-shm"):
    if os.path.exists(src + ext):
        shutil.copy(src + ext, dst + ext)
con = sqlite3.connect(dst)
def ft(v):
    try:
        return (datetime.datetime(1601, 1, 1) + datetime.timedelta(microseconds=int(v) / 10)).strftime("%H:%M:%S")
    except Exception:
        return str(v)
rows = con.execute("""
  SELECT n.Id, h.PrimaryId, n.Type, n.ArrivalTime, substr(n.Payload, 1, 160)
  FROM Notification n JOIN NotificationHandler h ON n.HandlerId = h.RecordId
  ORDER BY n.Id DESC LIMIT ?""", (n,)).fetchall()
for r in rows:
    payload = r[4].decode("utf-8", "replace") if isinstance(r[4], bytes) else str(r[4])
    print(f"{r[0]:>6} {ft(r[3])} {r[2]:<6} {r[1]}\n        {payload!r}")
print("--- handlers matching whatsapp/powershell ---")
for r in con.execute("SELECT RecordId, PrimaryId, HandlerType, CreatedTime FROM NotificationHandler WHERE PrimaryId LIKE '%whatsapp%' OR PrimaryId LIKE '%PowerShell%'"):
    print("  ", r)
