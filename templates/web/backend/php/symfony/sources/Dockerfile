FROM composer:2 AS vendor
WORKDIR /app
COPY composer.json ./
RUN composer install --no-dev --optimize-autoloader

FROM php:8.3-cli-alpine
RUN addgroup -S app && adduser -S -G app app
WORKDIR /app
COPY --from=vendor /app/vendor ./vendor
COPY . .
USER app
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/api/health || exit 1
CMD ["php", "-S", "0.0.0.0:3000", "-t", "/app/public"]
