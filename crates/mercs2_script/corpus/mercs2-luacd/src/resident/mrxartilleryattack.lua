function Create(uGuid, nShells, nDistance, sTemplate, nTime)
  nShells = nShells or 5
  
  nDistance = nDistance or 10
  sTemplate = sTemplate or "Artillery Shell"
  nTime = nTime or 4
  local x, y, z = Object.GetPosition(uGuid)
  if x then
    x = x + (math.randf() * nDistance - math.randf() * nDistance)
    z = z + (math.randf() * nDistance - math.randf() * nDistance)
    y = y + 250
    TriggerFallingMissile(x, y, z, "Artillery Smoke Shell")
    for i = 1, nShells do
      local nXAdjust = -(math.randf() * nDistance - math.randf() * nDistance + ((nShells + 1) / 2 - i) * nDistance)
      local nZAdjust = -(math.randf() * nDistance - math.randf() * nDistance)
      local TargetX = x + nXAdjust
      local TargetY = y
      local TargetZ = z + nZAdjust
      Event.Create(Event.TimerRelative, {
        5 + i * (nTime / nShells)
      }, TriggerFallingMissile, {
        TargetX,
        TargetY,
        TargetZ,
        sTemplate
      })
    end
  end
end

function TriggerFallingMissile(x, y, z, sTemplate)
  local uOrdnanceGuid = Airstrike.SpawnOrdnance(sTemplate, x, y, z, 0, -100, 0, "impact", 1)
end
