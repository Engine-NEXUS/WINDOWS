"""Tests for wake_data_fetch (local HTTP fixture; no network)."""
import http.server
import io
import tarfile
import tempfile
import threading
import unittest
import zipfile
from pathlib import Path
import sys

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wake_data_fetch as F  # noqa: E402

PAYLOAD = bytes(range(256)) * 4096          # 1 MiB


class Handler(http.server.BaseHTTPRequestHandler):
    truncate = False

    def log_message(self, *a):
        pass

    def do_GET(self):
        rng = self.headers.get("Range")
        start = int(rng.split("=")[1].split("-")[0]) if rng else 0
        body = PAYLOAD[start:]
        self.send_response(206 if rng else 200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body[: len(body) // 2] if Handler.truncate else body)


class Fetch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=cls.srv.serve_forever, daemon=True).start()
        cls.url = f"http://127.0.0.1:{cls.srv.server_address[1]}/f.bin"

    @classmethod
    def tearDownClass(cls):
        cls.srv.shutdown()

    def setUp(self):
        Handler.truncate = False

    def test_full_download_and_checksum_lock(self):
        with tempfile.TemporaryDirectory() as d:
            p = F.download(self.url, Path(d) / "f.bin", progress=False)
            self.assertEqual(p.read_bytes(), PAYLOAD)
            lock = Path(d) / F.LOCK
            h = F.sha256_file(p)
            self.assertEqual(F.check_lock(lock, "f.bin", h), "new")
            self.assertEqual(F.check_lock(lock, "f.bin", h), "ok")
            self.assertEqual(F.check_lock(lock, "f.bin", "deadbeef"), "MISMATCH")

    def test_resume_from_partial_file(self):
        with tempfile.TemporaryDirectory() as d:
            dest = Path(d) / "f.bin"
            dest.with_name("f.bin.part").write_bytes(PAYLOAD[:300000])
            F.download(self.url, dest, progress=False)
            self.assertEqual(dest.read_bytes(), PAYLOAD)

    def test_size_cap_refuses_before_downloading(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(ValueError):
                F.download(self.url, Path(d) / "f.bin", max_bytes=1000, progress=False)
            self.assertFalse((Path(d) / "f.bin").exists())

    def test_truncated_transfer_is_not_accepted_and_can_resume(self):
        with tempfile.TemporaryDirectory() as d:
            dest = Path(d) / "f.bin"
            Handler.truncate = True
            with self.assertRaises(IOError):
                F.download(self.url, dest, progress=False)
            self.assertFalse(dest.exists())
            Handler.truncate = False
            F.download(self.url, dest, progress=False)
            self.assertEqual(dest.read_bytes(), PAYLOAD)


class Extract(unittest.TestCase):
    def test_tar_path_traversal_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            arc = Path(d) / "evil.tar"
            with tarfile.open(arc, "w") as t:
                info = tarfile.TarInfo("../escape.txt")
                info.size = 2
                t.addfile(info, io.BytesIO(b"hi"))
            with self.assertRaises(ValueError):
                F.extract(arc, Path(d) / "out", "tar")
            self.assertFalse((Path(d) / "escape.txt").exists())

    def test_zip_extracts_inside_dest(self):
        with tempfile.TemporaryDirectory() as d:
            arc = Path(d) / "ok.zip"
            with zipfile.ZipFile(arc, "w") as z:
                z.writestr("a/b.txt", "x")
            F.extract(arc, Path(d) / "out", "zip")
            self.assertTrue((Path(d) / "out/a/b.txt").exists())

    def test_safe_member(self):
        self.assertTrue(F.safe_member("/x/y", "a/b"))
        self.assertFalse(F.safe_member("/x/y", "../z"))
        self.assertFalse(F.safe_member("/x/y", "/etc/passwd"))


class Audio(unittest.TestCase):
    def test_chunks_drop_silence(self):
        x = np.concatenate([np.full(32000, 0.2), np.zeros(32000), np.full(32000, 0.2)]).astype(np.float32)
        self.assertEqual(len(F.chunk_audio(x)), 2)

    def test_heldout_is_stable_and_roughly_a_fifth(self):
        names = [f"f{i}.wav" for i in range(2000)]
        flags = [F.is_heldout(n) for n in names]
        self.assertEqual(flags, [F.is_heldout(n) for n in names])
        self.assertAlmostEqual(sum(flags) / len(flags), 0.2, delta=0.04)

    def test_tv_mix_snr_and_peak(self):
        rng = np.random.default_rng(0)
        speech = rng.normal(0, 0.1, 16000).astype(np.float32)
        music = rng.normal(0, 0.1, 16000).astype(np.float32)
        out = F.make_tv_mix(speech, music, None, 10.0, rng)
        self.assertLessEqual(float(np.abs(out).max()), 0.6 + 1e-6)
        self.assertEqual(len(out), 16000)

    def test_datasets_are_all_permissive_and_have_attribution(self):
        for k, d in F.DATASETS.items():
            self.assertTrue(d["permissive"], k)
            self.assertTrue(d["attribution"], k)
            self.assertTrue(d["url"].startswith("http"), k)


if __name__ == "__main__":
    unittest.main()
