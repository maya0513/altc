# Copy可能な資源値

## 決定

v0ではすべてのstructとenumをcopy可能なvalueとします。pointerやallocationされたstorageをfieldへ含む型にも、move-only、affine、automatic drop、RAIIを導入しません。`alloc`と`free`は操作の存在を追跡するeffectであり、所有権を表しません。

## 理由

限定的なmove semanticsでも、generic、分岐、partial move、`defer`、C ABIへ広く影響します。borrow checkerを非目標とするaltcで一部だけを導入すると、保証と実装複雑性の境界が不明瞭になります。明示copyと`defer`による手動cleanupをCに近いbaselineとします。

## 帰結

leak、double free、use-after-freeはcompile-timeに防止しません。処理系はunused allocation resultなどをwarningにできますが、そのwarningを所有権保証として扱ってはなりません。
