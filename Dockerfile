FROM rust:1.100-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake libclang-dev unzip \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p voicebro-server

FROM build AS assets
WORKDIR /downloads
RUN curl -fsSL --retry 3 https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip -o model.zip \
    && echo '30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498  model.zip' | sha256sum -c - \
    && unzip -q model.zip -d /opt/assets
RUN curl -fsSL --retry 3 https://files.pythonhosted.org/packages/fc/ca/83398cfcd557360a3d7b2d732aee1c5f6999f68618d1645f38d53e14c9ff/vosk-0.3.45-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.whl -o vosk.whl \
    && echo '25e025093c4399d7278f543568ed8cc5460ac3a4bf48c23673ace1e25d26619f  vosk.whl' | sha256sum -c - \
    && unzip -j vosk.whl vosk/libvosk.so -d /opt/assets/vosk-linux64-0.3.45
RUN curl -fsSL --retry 3 https://registry.npmjs.org/@openai/codex/-/codex-0.167.0-linux-x64.tgz -o codex.tgz \
    && echo '2dc9aa871abb513c3539d20d025f701faf1974c46ee906bda3a1e1f5026d4434  codex.tgz' | sha256sum -c - \
    && mkdir -p /opt/codex \
    && tar xzf codex.tgz --strip-components=3 -C /opt/codex package/vendor/x86_64-unknown-linux-musl

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libstdc++6 curl libarchive-tools \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 voicebro \
    && install -d -o voicebro -g voicebro /data /home/voicebro/.codex \
    && ln -s /opt/assets /data/assets
COPY --from=build /src/target/release/voicebro-server /usr/local/bin/voicebro-server
COPY --from=assets /opt/assets /opt/assets
COPY --from=assets /opt/codex /opt/codex
RUN ln -s /opt/codex/bin/codex /usr/local/bin/codex && codex --version
ENV HOME=/home/voicebro VOICEBRO_DATA_DIR=/data
USER voicebro
WORKDIR /data
ENTRYPOINT ["voicebro-server"]
