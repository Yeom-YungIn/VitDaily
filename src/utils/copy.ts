export function friendlySystemText(value?: string | null): string {
  if (!value) return "";

  return [
    ["Live Order Gate", "실거래 보호장치"],
    ["Live Gate", "실거래 보호장치"],
    ["Safety Gate", "보호장치"],
    ["Global Live Lock", "전체 실거래 잠금"],
    ["Live Lock", "실거래 잠금"],
    ["Paper Trade", "모의 주문"],
    ["Paper", "모의 실행"],
    ["DCA", "나눠 사기"],
    ["Buy-and-hold", "처음에 전부 사기"],
    ["Buy/Hold", "처음에 전부 사기"],
    ["Gate", "보호장치"],
    ["Live", "실거래"],
  ].reduce((text, [from, to]) => text.split(from).join(to), value);
}
