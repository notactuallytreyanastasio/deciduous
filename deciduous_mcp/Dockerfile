# Two stages: the build image carries Elixir and the whole dep tree, the
# runtime image carries only the compiled release. Matches how the other
# Phoenix apps on this host are built.
# These three have to name a tag that actually exists on Docker Hub — hexpm
# publishes only specific elixir/erlang/debian combinations, and a plausible
# guess fails at `load metadata` with "not found". OTP 28 matches the version
# this is developed against.
ARG ELIXIR_VERSION=1.18.4
ARG OTP_VERSION=28.0
ARG DEBIAN_VERSION=bookworm-20260610-slim
ARG BUILDER_IMAGE="hexpm/elixir:${ELIXIR_VERSION}-erlang-${OTP_VERSION}-debian-${DEBIAN_VERSION}"
ARG RUNNER_IMAGE="debian:${DEBIAN_VERSION}"

FROM ${BUILDER_IMAGE} AS builder

RUN apt-get update -y && apt-get install -y build-essential git \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mix local.hex --force && mix local.rebar --force

ENV MIX_ENV=prod

COPY mix.exs mix.lock ./
RUN mix deps.get --only prod
RUN mix deps.compile

COPY config config
COPY lib lib
COPY priv priv

RUN mix compile
RUN mix release

FROM ${RUNNER_IMAGE}

RUN apt-get update -y \
    && apt-get install -y libstdc++6 openssl libncurses5 locales ca-certificates \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8 LANGUAGE=en_US:en LC_ALL=en_US.UTF-8

WORKDIR /app
RUN chown nobody /app

COPY --from=builder --chown=nobody:root /app/_build/prod/rel/deciduous_mcp ./

USER nobody

# Migrate, then run. A release has no Mix, so this is `eval`, not `mix`.
CMD ["/bin/sh", "-c", "/app/bin/deciduous_mcp eval 'DeciduousMcp.Release.migrate()' && exec /app/bin/deciduous_mcp start"]
