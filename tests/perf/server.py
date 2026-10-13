#!/usr/bin/env python3
"""A NETCONF server for benchmarks (TCP, as with --proto tcp).

  server.py --reply-mib N     every rpc is answered with a reply of about N MiB
  server.py                   small replies

It prints its port on the first line, then serves connections until killed. Edits and
other requests are read completely and discarded; close-session ends a connection.
"""
import argparse
import re
import socket
import threading

NC = "urn:ietf:params:xml:ns:netconf:base:1.0"
HELLO = (
    '<hello xmlns="%s"><capabilities><capability>urn:ietf:params:netconf:base:1.0</capability>'
    "<capability>urn:ietf:params:netconf:base:1.1</capability></capabilities>"
    "<session-id>1</session-id></hello>]]>]]>" % NC
).encode()
BLOCK = b"".join(
    b"<item><k>%d</k><v>abcdefghijklmnopqrstuvwxyz0123456789</v></item>\n" % i for i in range(1000)
)


class Reader:
    def __init__(self, conn):
        self.conn = conn
        self.buf = b""

    def fill(self):
        data = self.conn.recv(1 << 20)
        if not data:
            raise EOFError
        self.buf += data

    def until(self, marker):
        while marker not in self.buf:
            self.fill()
        head, _, self.buf = self.buf.partition(marker)
        return head

    def exactly(self, n):
        """Reads n bytes, returning them only when small: big payloads are discarded."""
        kept = b""
        while n > 0:
            if not self.buf:
                self.fill()
            take, self.buf = self.buf[:n], self.buf[n:]
            if len(kept) < 4096:
                kept += take[: 4096 - len(kept)]
            n -= len(take)
        return kept

    def chunked_message(self):
        """Reads one chunked message; returns its first bytes."""
        head = b""
        while True:
            self.until(b"\n")  # the newline before the header
            header = self.until(b"\n")  # '#<size>' or '##'
            if header == b"##":
                return head
            head += self.exactly(int(header[1:]))


def reply(conn, message_id, megabytes):
    prefix = ('<rpc-reply xmlns="%s" message-id="%s"><data>' % (NC, message_id)).encode()
    suffix = b"</data></rpc-reply>"
    total = megabytes << 20
    conn.sendall(b"\n#%d\n" % len(prefix) + prefix)
    sent = 0
    while sent < total:
        # Whole elements only, so that the reply is well-formed XML.
        conn.sendall(b"\n#%d\n" % len(BLOCK) + BLOCK)
        sent += len(BLOCK)
    conn.sendall(b"\n#%d\n" % len(suffix) + suffix + b"\n##\n")


def serve(conn, megabytes):
    with conn:
        reader = Reader(conn)
        try:
            reader.until(b"\n")  # the IPC preamble
            conn.sendall(HELLO)
            reader.until(b"]]>]]>")
            while True:
                head = reader.chunked_message()
                match = re.search(rb'message-id="([^"]*)"', head)
                ident = match.group(1).decode() if match else "1"
                if b"close-session" in head:
                    reply(conn, ident, 0)
                    return
                if b"edit-config" in head or b"copy-config" in head:
                    reply(conn, ident, 0)
                else:
                    reply(conn, ident, megabytes)
        except (EOFError, ConnectionError):
            return


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reply-mib", type=int, default=0)
    options = parser.parse_args()
    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(8)
    print(listener.getsockname()[1], flush=True)
    while True:
        conn, _ = listener.accept()
        threading.Thread(target=serve, args=(conn, options.reply_mib), daemon=True).start()


if __name__ == "__main__":
    main()
