function Multi(tObjects)
  if not tObjects then
    Debug.Printf([[

Multi( { template1 [,template2] [,template3] [...] } )
Spawns multiple templates 10m in front of the camera]])
    return
  end
  for i, object in ipairs(tObjects) do
    Pg.SpawnFromCamera(object, 10, 0.5)
  end
end

function Scatter(sObject, nNumber, nOffset, nTime, nDistance, nHeight)
  nOffset = nOffset or 0
  nTime = nTime or 0
  nNumber = nNumber or 1
  nDistance = nDistance or 10
  nHeight = nHeight or 0.5
  if not sObject then
    Debug.Printf([[
Scatter( template [,number] [,radius] [,time] [,distance] [,height])
Example:
scatter( 'assault rifle' , 3 , 0 , 6 )
will stack 3 assault rifles, one every two seconds]])
    return
  end
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(nDistance, nHeight)
  Pg.Spawn(sObject, nSpawnX, nSpawnY, nSpawnZ)
  for i = 1, nNumber - 1 do
    local x = nSpawnX + (math.randf() * nOffset - math.randf() * nOffset) / 2
    local z = nSpawnZ + (math.randf() * nOffset - math.randf() * nOffset) / 2
    Event.Create(Event.TimerRelative, {
      nTime / nNumber * i
    }, Pg.Spawn, {
      sObject,
      x,
      nSpawnY,
      z
    })
  end
end
