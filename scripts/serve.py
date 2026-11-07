#!/usr/bin/env python3
"""Tiny static server with SPA fallback (for the stable ChefDAV instance)."""
import http.server
import os
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        path = self.translate_path(self.path)
        if not os.path.exists(path) or os.path.isdir(path):
            self.path = "/index.html"
        return super().do_GET()

    def log_message(self, fmt, *args):
        # keep the terminal quiet; uncomment for debugging
        pass


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 3200
    directory = sys.argv[2] if len(sys.argv) > 2 else "."
    os.chdir(directory)
    http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    main()
