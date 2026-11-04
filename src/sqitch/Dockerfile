# Schema migration image: sqitch plus this release's core project.
#
# Published by release-build.yml under the same tag as the services, so a
# deployment repository can run the schema for exactly the release it is
# about to roll out (e.g. as an ArgoCD PreSync hook) and move both with one
# tag bump. demo-data is deliberately left out: an installation with its
# own data never deploys it.
#
#   docker run --rm -e DATABASE_URL=postgres://... <image> [sqitch deploy args]
#
# Extra arguments go to `sqitch deploy`, e.g. odo's deploy-time variables:
#   --set root_code=KCLS --set root_label=KCLS --set root_uuid=<uuid>

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    sqitch \
    libdbd-pg-perl \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1010 appuser

COPY src/sqitch/core/ /sqitch/
COPY --chmod=0755 src/sqitch/migrate.sh /usr/local/bin/migrate

# sqitch records a committer on every change; there is no meaningful
# user in a container.
ENV SQITCH_FULLNAME="odo migrate" \
    SQITCH_EMAIL="odo-migrate@localhost"

USER appuser
WORKDIR /sqitch

ENTRYPOINT ["/usr/local/bin/migrate"]
