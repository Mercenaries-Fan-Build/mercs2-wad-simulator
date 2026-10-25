# The profiler's capture service: tracy-capture + tracy-csvexport, built from the Tracy release the
# profiler client links, and mercs2_profd, which runs tracy-capture for each session.
# TRACY_TAG must match the Tracy version bundled by the client's tracy-client-sys
# (tracy/common/TracyVersion.hpp): the capture protocol is checked for an exact match.
#
# Build context: the workspace root (compose.yaml sets it); Dockerfile.dockerignore keeps targets out.
FROM debian:trixie-slim AS tracy

ARG TRACY_TAG=v0.14.1
ARG TRACY_COMMIT=30997d5ca6bb632cc10807a1da8a6d3de0aeeb3c

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git cmake ninja-build g++ pkg-config patch \
 && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 --branch "${TRACY_TAG}" https://github.com/wolfpld/tracy.git /src/tracy \
 && test "$(git -C /src/tracy rev-parse HEAD)" = "${TRACY_COMMIT}"

RUN cmake -S /src/tracy/capture -B /build/capture -G Ninja -DCMAKE_BUILD_TYPE=Release -DNO_ISA_EXTENSIONS=ON \
 && cmake --build /build/capture \
 && cmake -S /src/tracy/csvexport -B /build/csvexport -G Ninja -DCMAKE_BUILD_TYPE=Release -DNO_ISA_EXTENSIONS=ON \
 && cmake --build /build/csvexport

FROM rust:1.94.1-slim-trixie AS profd
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY xtask xtask
RUN cargo build --release --locked -p mercs2_profd

FROM debian:trixie-slim
COPY --from=tracy /build/capture/tracy-capture /usr/local/bin/tracy-capture
COPY --from=tracy /build/csvexport/tracy-csvexport /usr/local/bin/tracy-csvexport
COPY --from=profd /src/target/release/mercs2_profd /usr/local/bin/mercs2_profd
ENV PROFD_TRACY_CAPTURE=/usr/local/bin/tracy-capture
ENV PROFD_CAPTURES=/captures
WORKDIR /captures
CMD ["mercs2_profd"]
