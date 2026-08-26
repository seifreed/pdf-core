FROM gcr.io/oss-fuzz-base/base-builder-rust

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . $SRC/pdf-core
COPY .clusterfuzzlite/build.sh $SRC/
WORKDIR $SRC/pdf-core
