# Shared Resources Folder

The `shared/` directory contains code that is reused across both the **web** assets and the **app** logic, such as:

- Type definitions and interfaces.
- Utility functions (e.g., formatting, validation).
- Constants, enums, and configuration objects.
- Common React components or hooks that are not tied to a specific page.

Placing these items here prevents duplication and makes it easy to import them from either `web/` or `app/`.
