/** 剥掉 TOML 源码形式值的首尾引号与空白。
 *  后端 model_values 存的是 toml_edit 的 Item Display（源码形式），带引号和前导空格，
 *  展示给用户前需要还原成纯值。 */
export function stripTomlQuotes(value: string | null | undefined): string {
  return (value ?? "").trim().replace(/^["'`]+|["'`]+$/g, "").trim();
}
