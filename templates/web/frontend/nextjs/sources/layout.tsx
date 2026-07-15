import "../styles/theme.css";
import type { ReactNode } from "react";

export const metadata = {
    title: "{{project_name}}",
    description: "MegaGate frontend scaffold",
    icons: {
        icon: "/favicon.ico",
    },
};

export default function RootLayout({ children }: { children: ReactNode }) {
    return (
        <html lang="en">
            <body>{children}</body>
        </html>
    );
}
