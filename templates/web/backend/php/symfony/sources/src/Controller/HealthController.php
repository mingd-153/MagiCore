<?php

namespace App\Controller;

use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;

class HealthController extends AbstractController
{
    public function index(): Response
    {
        return $this->json([
            'service' => '{{project_name}}',
            'framework' => 'symfony',
            'message' => '{{project_name}} backend scaffold ready',
        ]);
    }

    public function health(): JsonResponse
    {
        return $this->json([
            'status' => 'ok',
            'service' => '{{project_name}}',
            'timestamp' => (new \DateTimeImmutable())->format(\DateTimeInterface::ATOM),
        ]);
    }
}
