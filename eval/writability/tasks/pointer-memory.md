# pointer・nullability・memory effect

Cのallocatorが返すnullable pointerをpattern matchし、non-null branchで型付きpointerへcastする。
1 byteを書いて読み戻し、すべての退出でdeferにより解放する。memory summaryには間接read、
間接write、extern callが現れなければならない。
