export class AppRouter {
  static routes = {
    "/": () => import("../pages/Home"),
  };

  static async navigate(path: string) {
    const route = this.routes[path];
    if (route) {
      const module = await route();
      return module.default();
    }
  }
}