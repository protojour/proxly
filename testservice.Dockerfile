FROM scratch AS dist
COPY target-musl/x86_64-unknown-linux-musl/debug/proxly-testservice /proxly-testservice

ENTRYPOINT ["/proxly-testservice"]
CMD ["--help"]
