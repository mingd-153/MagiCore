<?php
require __DIR__ . '/vendor/autoload.php';

use Slim\Factory\AppFactory;

$app = AppFactory::create();
$app->get('/api/health', function ($request, $response) {
    $response->getBody()->write(json_encode(['status' => 'ok']));
    return $response->withHeader('Content-Type', 'application/json');
});
$app->run();
