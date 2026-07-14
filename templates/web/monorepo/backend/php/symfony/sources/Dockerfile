FROM composer:2 AS composer
WORKDIR /app
COPY composer.json ./
RUN composer install --no-dev

FROM php:8.3-cli-alpine
WORKDIR /app
COPY --from=composer /app/vendor ./vendor
COPY . .
EXPOSE 3000
CMD ["php", "-S", "0.0.0.0:3000", "-t", "/app"]
