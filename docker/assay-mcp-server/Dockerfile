# Copies already-built linux assay-mcp-server binaries. No RUN: BuildKit's
# TARGETARCH selects the file, so the image does not rebuild or invoke QEMU.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:54df941ed0d06a1bd95ef5e0ce391fd8d9f94b64782dc9a60062727849ee3f97

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
