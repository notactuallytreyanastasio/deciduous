# Standard OTP release. Dockerfile.burrito separately builds the single-file
# Linux artifact, so this path does not need Zig even with legacy Docker.
# These three have to name a tag that actually exists on Docker Hub — hexpm
# publishes only specific elixir/erlang/debian combinations, and a plausible
# guess fails at `load metadata` with "not found". OTP 28 matches the version
# this is developed against.
ARG ELIXIR_VERSION=1.18.4
ARG OTP_VERSION=28.3.3
ARG DEBIAN_VERSION=bookworm-20260610-slim
ARG BUILDER_IMAGE="hexpm/elixir:${ELIXIR_VERSION}-erlang-${OTP_VERSION}-debian-${DEBIAN_VERSION}"
ARG RUNNER_IMAGE="debian:${DEBIAN_VERSION}"

FROM ${BUILDER_IMAGE} AS compile

RUN apt-get update -y && apt-get install -y build-essential git \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mix local.hex --force && mix local.rebar --force

ENV MIX_ENV=prod

COPY mix.exs mix.lock ./
COPY rel rel
# hermes_mcp is vendored (a path dependency) so deps.get needs it before it runs.
COPY vendor vendor
RUN mix deps.get --only prod
RUN mix deps.compile

COPY config config
COPY lib lib
COPY priv priv

RUN mix compile

FROM compile AS builder
RUN mix release deciduous_mcp

FROM ${RUNNER_IMAGE} AS runtime

RUN apt-get update -y \
    && apt-get install -y libstdc++6 openssl libncurses5 locales ca-certificates curl \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8 LANGUAGE=en_US:en LC_ALL=en_US.UTF-8

WORKDIR /app
RUN chown nobody /app

USER nobody
EXPOSE 4000
HEALTHCHECK --interval=5s --timeout=3s --start-period=30s --retries=12 \
    CMD curl --fail --silent "http://127.0.0.1:${PORT:-4000}/ready" || exit 1

COPY --from=builder --chown=nobody:root /app/_build/prod/rel/deciduous_mcp ./
COPY scripts/docker-entrypoint.sh /app/docker-entrypoint.sh

ENTRYPOINT ["/bin/sh", "/app/docker-entrypoint.sh"]
