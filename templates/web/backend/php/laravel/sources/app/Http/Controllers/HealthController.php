<?php

namespace App\Http\Controllers;

use Illuminate\Http\JsonResponse;

class HealthController extends Controller
{
    public function index(): JsonResponse
    {
        return response()->json([
            'status' => 'ok',
            'service' => '{{project_name}}',
            'timestamp' => now()->toIso8601String(),
        ]);
    }
}
