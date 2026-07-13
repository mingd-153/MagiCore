import "../styles/theme.css";

export const metadata = {
    title: "{{project_name}}",
    description: "MegaGate frontend scaffold",
    icons: {
        icon: "/favicon.ico",
    },
};

export default function RootLayout({ children }) {
    return (
        <html lang="en">
            <body>{children}</body>
        </html>
    );
}
