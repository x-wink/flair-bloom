interface RuleDeletionRequest {
  id: string;
  force?: boolean;
  confirm: () => Promise<boolean>;
  remove: (id: string) => void;
}

export async function requestRuleDeletion({
  id,
  force = false,
  confirm,
  remove,
}: RuleDeletionRequest): Promise<void> {
  if (force || (await confirm())) remove(id);
}
