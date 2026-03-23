import { describe, test, expect } from "vitest";
import {
  shortCpuName,
  shortGpuName,
  formatSize,
  quantColor,
  quantSortKey,
  isImatrixFile,
} from "./format";

describe("shortCpuName", () => {
  test("Intel Core i7", () => {
    expect(shortCpuName("Intel(R) Core(TM) i7-9700K CPU @ 3.60GHz")).toBe("i7-9700K");
  });

  test("Intel Core i9", () => {
    expect(shortCpuName("Intel(R) Core(TM) i9-13900K")).toBe("i9-13900K");
  });

  test("AMD Ryzen", () => {
    expect(shortCpuName("AMD Ryzen 9 7950X 16-Core Processor")).toBe("Ryzen 9 7950X");
  });

  test("Apple M3", () => {
    expect(shortCpuName("Apple M3 Max")).toBe("Apple M3");
  });

  test("Intel Xeon", () => {
    expect(shortCpuName("Intel Xeon E5-2680 v4")).toBe("Xeon E5-2680");
  });

  test("Intel Xeon with (R) markers", () => {
    expect(shortCpuName("Intel(R) Xeon(R) E5-2680 v4 @ 2.40GHz")).toBe("Xeon E5-2680");
  });

  test("fallback strips CPU suffix", () => {
    expect(shortCpuName("Some Unknown CPU @ 2.0GHz")).toBe("Some Unknown");
  });
});

describe("shortGpuName", () => {
  test("NVIDIA RTX 4090", () => {
    expect(shortGpuName("NVIDIA GeForce RTX 4090")).toBe("RTX 4090");
  });

  test("NVIDIA RTX 3080", () => {
    expect(shortGpuName("NVIDIA GeForce RTX 3080")).toBe("RTX 3080");
  });

  test("NVIDIA RTX 4070 Ti", () => {
    expect(shortGpuName("NVIDIA GeForce RTX 4070 Ti")).toBe("RTX 4070 Ti");
  });

  test("AMD RX 7900 XTX", () => {
    expect(shortGpuName("AMD Radeon RX 7900 XTX")).toBe("RX 7900 XTX");
  });

  test("Intel Arc A770", () => {
    expect(shortGpuName("Intel Arc A770")).toBe("Arc A770");
  });

  test("fallback strips vendor prefix", () => {
    expect(shortGpuName("NVIDIA Something Custom")).toBe("Something Custom");
  });
});

describe("formatSize", () => {
  test("large GB value", () => {
    expect(formatSize(4.7 * 1024 ** 3)).toBe("4.7 GB");
  });

  test("1 GB exactly", () => {
    expect(formatSize(1024 ** 3)).toBe("1.0 GB");
  });

  test("MB value shows 2 decimals", () => {
    expect(formatSize(512 * 1024 ** 2)).toBe("512.00 MB");
  });

  test("small MB value", () => {
    expect(formatSize(100.5 * 1024 ** 2)).toBe("100.50 MB");
  });

  test("zero bytes", () => {
    expect(formatSize(0)).toBe("0.00 MB");
  });
});

describe("quantColor", () => {
  test("F16/BF16/F32 are blue", () => {
    expect(quantColor("F16")).toBe("badge-blue");
    expect(quantColor("BF16")).toBe("badge-blue");
    expect(quantColor("F32")).toBe("badge-blue");
  });

  test("Q8 is blue", () => {
    expect(quantColor("Q8_0")).toBe("badge-blue");
  });

  test("Q6 is cyan", () => {
    expect(quantColor("Q6_K")).toBe("badge-cyan");
  });

  test("Q5 is green", () => {
    expect(quantColor("Q5_K_S")).toBe("badge-green");
  });

  test("Q4 is yellow", () => {
    expect(quantColor("Q4_K_M")).toBe("badge-yellow");
  });

  test("Q3 is orange", () => {
    expect(quantColor("Q3_K_M")).toBe("badge-orange");
  });

  test("Q2 is red", () => {
    expect(quantColor("Q2_K")).toBe("badge-red");
  });

  test("Q1 is dark red", () => {
    expect(quantColor("Q1_0")).toBe("badge-red-dark");
  });

  test("IQ treated same as Q", () => {
    expect(quantColor("IQ4_XS")).toBe("badge-yellow");
    expect(quantColor("IQ2_XXS")).toBe("badge-red");
    expect(quantColor("IQ3_M")).toBe("badge-orange");
  });

  test("MXFP4 treated as 4-bit", () => {
    expect(quantColor("MXFP4")).toBe("badge-yellow");
  });

  test("unknown is gray", () => {
    expect(quantColor("UNKNOWN")).toBe("badge-gray");
  });
});

describe("quantSortKey", () => {
  test("ordering is F32 < BF16 < F16 < Q8 < Q6 < Q4 < Q3 < Q2 < Q1", () => {
    const keys = ["F32", "BF16", "F16", "Q8_0", "Q6_K", "Q4_K_M", "Q3_K_M", "Q2_K", "Q1_0"].map(quantSortKey);
    for (let i = 1; i < keys.length; i++) {
      expect(keys[i]).toBeGreaterThanOrEqual(keys[i - 1]);
    }
  });

  test("null returns 999", () => {
    expect(quantSortKey(null)).toBe(999);
  });

  test("IQ has same rank as Q", () => {
    expect(quantSortKey("IQ4_XS")).toBe(quantSortKey("Q4_K_M"));
    expect(quantSortKey("IQ2_XXS")).toBe(quantSortKey("Q2_K"));
  });
});

describe("isImatrixFile", () => {
  test("detects imatrix files", () => {
    expect(isImatrixFile("model-imatrix.gguf")).toBe(true);
    expect(isImatrixFile("IQuest-Coder-V1-40B-Instruct-imatrix.gguf")).toBe(true);
    expect(isImatrixFile("model-importance_matrix.gguf")).toBe(true);
  });

  test("does not match regular model files", () => {
    expect(isImatrixFile("model-Q4_K_M.gguf")).toBe(false);
    expect(isImatrixFile("Llama-3.1-8B-F16.gguf")).toBe(false);
  });
});
