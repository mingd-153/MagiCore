<?php

namespace App;

use Symfony\Bundle\FrameworkBundle\FrameworkBundle;
use Symfony\Bundle\FrameworkBundle\Kernel\MicroKernelTrait;
use Symfony\Component\Config\Loader\LoaderInterface;
use Symfony\Component\HttpKernel\Kernel as BaseKernel;
use Symfony\Component\Routing\Loader\Configurator\RoutingConfigurator;

class Kernel extends BaseKernel
{
    use MicroKernelTrait;

    public function registerBundles(): iterable
    {
        yield new FrameworkBundle();
    }

    public function registerContainerConfiguration(LoaderInterface $loader): void
    {
        $loader->load(dirname(__DIR__).'/config/packages/framework.yaml');
        $loader->load(dirname(__DIR__).'/config/services.yaml');
    }

    protected function configureRoutes(RoutingConfigurator $routes): void
    {
        $routes->import(dirname(__DIR__).'/config/routes.yaml');
    }
}
