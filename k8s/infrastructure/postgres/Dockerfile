FROM postgres:17-trixie

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        postgresql-17-pgtap \
        postgresql-contrib \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
