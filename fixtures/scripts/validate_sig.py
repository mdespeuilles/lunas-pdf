"""Oracle : valide les signatures d'un PDF avec pyHanko (intégrité + couverture)."""
import sys
from pyhanko.pdf_utils.reader import PdfFileReader
from pyhanko.sign.validation import validate_pdf_signature
from pyhanko_certvalidator import ValidationContext
from pyhanko.keys import load_cert_from_pemder

vc = ValidationContext(trust_roots=[load_cert_from_pemder(sys.argv[2])]) if len(sys.argv) > 2 and sys.argv[2] != "-v" else None
with open(sys.argv[1], "rb") as f:
    r = PdfFileReader(f, strict=False)
    for s in r.embedded_signatures:
        st = validate_pdf_signature(s, vc)
        print(f"{s.field_name}: intact={st.intact} valid={st.valid} trusted={st.trusted} "
              f"coverage={st.coverage.name} modification={st.modification_level.name if st.modification_level else None} "
              f"docmdp_ok={st.docmdp_ok}")
        if "-v" in sys.argv:
            print("   diff_result:", st.diff_result)
