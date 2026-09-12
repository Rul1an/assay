# Copies already-built linux assay-mcp-server binaries. No RUN: BuildKit's
# TARGETARCH selects the file, so the image does not rebuild or invoke QEMU.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:c31ff9abcb1910f3ab25c7957bdaf0bfe12a01eb546e8df2282f1c8f682b606c

ARG VERSION=unknown
ARG TARGETARCH

LABEL org.opencontainers.image.source="https://github.com/Rul1an/assay" \
      org.opencontainers.image.url="https://github.com/Rul1an/assay" \
      org.opencontainers.image.title="assay-mcp-server" \
      org.opencontainers.image.description="MCP policy-server for Assay (stdio; no proxy child in this image)" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}" \
      io.modelcontextprotocol.server.name="io.github.Rul1an/assay-mcp-server"

COPY --chmod=0755 binaries/${TARGETARCH}/assay-mcp-server /usr/local/bin/assay-mcp-server

USER 65532:65532

ENTRYPOINT ["/usr/local/bin/assay-mcp-server"]
