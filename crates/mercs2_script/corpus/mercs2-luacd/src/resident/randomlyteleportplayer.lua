local tLocation = {
  "All_HQ",
  "Gur_HQ2",
  "GurHQ",
  "MecRecruit",
  "OilHQ",
  "PMC1.1",
  "Teleporter 0x000921c5"
}
local x, y, z, n, s, e, guid

function Init()
  Go()
end

function Go()
  s = tLocation[Math.floor(Math.randf(1, table.maxn(tLocation)))]
  guid = Pg.GetGuidByName(s)
  if guid ~= nil then
    x, y, z = Object.GetPosition(guid)
  else
    x, y, z = nil, nil, nil
  end
  if x == nil then
    Debug.Printf("STRESS TEST: Bad location: " .. tostring(s))
    e = Event.Create(Event.TimerRelative, {0.1}, Go, nil)
  else
    n = Math.randf(5, 20)
    Debug.Printf("STRESS TEST: going to: " .. tostring(s) .. ". Teleporting in " .. tostring(n))
    Object.SetPosition(Player.GetLocalCharacter(), x, y + 20, z)
    e = Event.Create(Event.TimerRelative, {
      Math.randf(5, 20)
    }, Go, nil)
  end
end
