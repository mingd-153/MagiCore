import { Lucia } from "lucia";
import { PrismaAdapter } from "@lucia-auth/adapter-prisma";
import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();
const adapter = new PrismaAdapter(prisma.session, prisma.user);

export const lucia = new Lucia(adapter, {
  sessionCookie: {
    attributes: { secure: process.env.NODE_ENV === "production" },
  },
  getUserAttributes: (attributes) => ({ email: attributes.email }),
});