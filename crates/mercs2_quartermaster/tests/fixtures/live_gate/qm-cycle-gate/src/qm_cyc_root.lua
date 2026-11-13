-- The gate's root: a library qm's loader brings up with dynamic_import, which queues it and, through
-- its DEPS, qm_cyc_a and qm_cyc_b.
import("qm_cyc_a")
print("[qmcyc] root body: a=" .. tostring(qm_cyc_a) .. " a.ran=" .. tostring(qm_cyc_a and qm_cyc_a.ran))
ran = true

function Init()
    print("[qmcyc] root Init: a.ran=" .. tostring(qm_cyc_a and qm_cyc_a.ran))
end
