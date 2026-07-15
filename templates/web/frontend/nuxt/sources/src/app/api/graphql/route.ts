import { graphql } from "graphql";
import { schema } from "@/lib/graphql/schema";

export async function POST(req: Request) {
  const { query, variables } = await req.json();
  const result = await graphql({ schema, source: query, variableValues: variables });
  return Response.json(result);
}