<?php

use Illuminate\Support\Facades\Route;
use App\Http\Controllers\HealthController;

Route::get('/', function () {
    return response()->json([
        'service' => '{{project_name}}',
        'framework' => 'laravel',
        'message' => '{{project_name}} backend scaffold ready',
    ]);
});

Route::get('/health', [HealthController::class, 'index']);
