/**
 * Common Utility Types
 * Reusable utility types for the UI
 */

export type Nullable<T> = T | null;

export type Optional<T> = T | undefined;

export type NullableOptional<T> = T | null | undefined;

export type Dictionary<T> = Record<string, T>;

export type StringKey<T> = Extract<keyof T, string>;

export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

export type DeepReadonly<T> = {
  readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
};

export type RequireAtLeastOne<T, Keys extends keyof T = keyof T> =
  Pick<T, Exclude<keyof T, Keys>> &
  {
    [K in Keys]-?: Required<Pick<T, K>> & Partial<Pick<T, Exclude<Keys, K>>>;
  }[Keys];

export type RequireOnlyOne<T, Keys extends keyof T = keyof T> =
  Pick<T, Exclude<keyof T, Keys>> &
  {
    [K in Keys]-?: Required<Pick<T, K>> & Partial<Record<Exclude<Keys, K>, never>>;
  }[Keys];

export type ValueOf<T> = T[keyof T];

export type Entries<T> = {
  [K in keyof T]: [K, T[K]];
}[keyof T][];

export type PickByValue<T, ValueType> = Pick<
  T,
  { [Key in keyof T]-?: T[Key] extends ValueType ? Key : never }[keyof T]
>;

export type OmitByValue<T, ValueType> = Pick<
  T,
  { [Key in keyof T]-?: T[Key] extends ValueType ? never : Key }[keyof T]
>;

export type Writable<T> = {
  -readonly [P in keyof T]: T[P];
};

export type MaybePromise<T> = T | Promise<T>;

export type AsyncReturnType<T extends (...args: any) => Promise<any>> =
  T extends (...args: any) => Promise<infer R> ? R : any;

export type UnionToIntersection<U> =
  (U extends any ? (k: U) => void : never) extends ((k: infer I) => void) ? I : never;

export type Pretty<T> = {
  [K in keyof T]: T[K];
} & {};

export interface WithId {
  id: string;
}

export interface WithTimestamps {
  createdAt: Date;
  updatedAt: Date;
}

export interface WithSoftDelete {
  deletedAt?: Date;
  isDeleted: boolean;
}

export interface WithTenant {
  tenantId: string;
}

export interface WithOwner {
  ownerId: string;
  ownerType: 'user' | 'system' | 'service';
}

export interface WithVersion {
  version: number;
}

export interface WithMetadata {
  metadata?: Record<string, any>;
}

export type EntityBase = WithId & WithTimestamps;

export type TenantEntity = EntityBase & WithTenant;

export type OwnedEntity = EntityBase & WithOwner;

export type VersionedEntity = EntityBase & WithVersion;

export type FullEntity = TenantEntity & OwnedEntity & WithSoftDelete & WithMetadata;
