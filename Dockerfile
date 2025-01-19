FROM golang:1.23-bookworm AS base

WORKDIR /opt/app

# RUN apt-get update 
# RUN apt-get install -y just

# create ~/bin
RUN mkdir -p ~/bin

# download and extract just to ~/bin/just
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /opt/bin

# add `~/bin` to the paths that your shell searches for executables
# this line should be added to your shells initialization file,
# e.g. `~/.bashrc` or `~/.zshrc`
ENV PATH="$PATH:/opt/bin"

# just should now be executable
RUN just --help


COPY ./go.* .
RUN go mod download
RUN go mod tidy

# RUN apk add --no-cache make
COPY Justfile .

FROM base AS prod-builder

WORKDIR /opt/app

RUN apt-get update && apt-get install -y dumb-init

COPY .air.toml .
COPY ./cmd/ .
RUN rm -rf ./tmp
RUN go mod tidy

RUN just full-build

FROM gcr.io/distroless/base-debian12 AS prod

EXPOSE 8080
WORKDIR /opt/app

COPY --from=prod-builder /usr/bin/dumb-init /usr/bin/dumb-init
COPY --from=prod-builder /opt/app/bin/main /opt/app/main

USER nonroot:nonroot

USER nonroot:nonroot
ENTRYPOINT ["/usr/bin/dumb-init", "--"]
CMD ["./main"]

FROM base AS dev

RUN apt-get update && apt-get install -y iputils-ping

EXPOSE 8080
WORKDIR /opt/app

COPY .air.toml .
COPY ./cmd/ .

RUN go install github.com/air-verse/air@latest
RUN go mod tidy

CMD ["just", "watch"]
