/** The people of the organisation and their access tokens: `/v1/people` and `/v1/tokens`. */
import { del, get, patch, post } from "../http";
import type { AccessToken, AddedPerson, IssuedToken, Person } from "../types/server";

export type Role = Person["role"];

/** Every person, disabled ones included. */
export function people(): Promise<Person[]> {
  return get<Person[]>("/v1/people");
}

/** An owner adds a person and gets their first token to hand over. */
export function addPerson(person: {
  handle: string;
  name: string;
  role: Role;
}): Promise<AddedPerson> {
  return post<AddedPerson>("/v1/people", person);
}

/** An owner changes a person's role or disables or enables them. */
export function changePerson(
  id: number,
  change: { role?: Role; disabled?: boolean },
): Promise<Person> {
  return patch<Person>(`/v1/people/${id}`, change);
}

/** A person's tokens; an owner may ask for anyone's. */
export function tokensOf(person: number): Promise<AccessToken[]> {
  return get<AccessToken[]>(`/v1/people/${person}/tokens`);
}

/** The tokens of the person asking. */
export function ownTokens(): Promise<AccessToken[]> {
  return get<AccessToken[]>("/v1/tokens");
}

/** A new token for the person asking. */
export function newOwnToken(name: string): Promise<IssuedToken> {
  return post<IssuedToken>("/v1/tokens", { name });
}

export function revokeToken(id: number): Promise<AccessToken> {
  return del<AccessToken>(`/v1/tokens/${id}`);
}
