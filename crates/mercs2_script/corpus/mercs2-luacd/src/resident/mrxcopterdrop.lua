import("MrxSupport")
sDeliveryVehicle = "UH1 Transport (PMC) (Driver)"
sCargoToDeliver = "box"
uCargoToDeliver = Pg.GetGuidByName(sCargoToDeliver)
nAltitude = 25

function Create(sFaction, sCargo, nDesX, nDesY, nDesZ, bCareless, nTargetX, nTargetY, nTargetZ)
  if Net.IsClient() then
    return
  end
  local uCargoTemplate = Pg.GetGuidByName(sCargo)
  if not uCargoTemplate then
    Debug.Printf("ERROR in MrxCopterDrop.Crate. Could not find cargo \"" .. tostring(sCargo) .. "\" ,,, Is the name being exported?")
  end
  local tCopterData = {
    AL = "MH53J (Driver)",
    CH = "Ka29b (Driver)",
    GR = "UH1 Transport (GR) (Driver)",
    OC = "Coanda Transport (Driver)",
    PR = "Alouette3 Transport (PR) (Driver)",
    VZ = "Alouette3 Transport (VZ) (Driver)",
    VZH = "Mi26 (VZ) (Driver)",
    VZHF = "Mi26 (VZA Intro) (Driver)",
    VZF = "Alouette3 Transport (VZA Intro) (Driver)"
  }
  local uCopterTemplate = Pg.GetGuidByName(tCopterData[sFaction]) or Pg.GetGuidByName("Ka29b (Driver)")
  local nSpawnDistance = Object.GetHibernationDistance(uCargoTemplate) or 200
  nSpawnDistance = Math.min(nSpawnDistance - 5, 200)
  local uCargo = Pg.SpawnFromCamera(uCargoTemplate, nSpawnDistance, 1, true, Player.GetLocalCharacter(), false, true)
  if not uCargo then
    Debug.Printf("DELIVERY ERROR: No cargo spawned")
    return
  end
  if not nTargetX then
    nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(nSpawnDistance, nAltitude, 10)
    if nDesY and nTargetY < nDesY + nAltitude then
      nTargetY = nDesY + nAltitude
    end
  end
  local uHeli = Pg.Spawn(uCopterTemplate, nTargetX, nTargetY, nTargetZ, 0, false, true)
  if not uHeli then
    Object.Remove(uCargo)
    Debug.Printf("DELIVERY ERROR: No copter spawned")
    return
  end
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _DeployWinch, {
    uHeli,
    uCargo,
    nDesX,
    nDesY,
    nDesZ,
    bCareless
  })
  return uHeli, uCargo
end

function _DeployWinch(uHeli, uCargo, nDesX, nDesY, nDesZ, bCareless)
  Debug.Printf("---------- Copter is awake")
  Object.SetWinchState(uHeli, "deployed")
  Event.Create(Event.TimerRelative, {0.1}, Event.Create, {
    Event.ObjectHibernation,
    {uCargo, "awake"},
    _WaitCallback,
    {
      uHeli,
      uCargo,
      nDesX,
      nDesY,
      nDesZ,
      bCareless
    }
  })
end

function _WaitCallback(uHeli, uCargo, nDesX, nDesY, nDesZ, bCareless)
  Debug.Printf("---------- Cargo is awake")
  Object.SetYaw(uCargo, Object.GetYaw(uHeli))
  Object.AttachCargoToWinch(uCargo, uHeli)
  Ai.Deliver(Vehicle.GetDriver(uHeli), nDesX, nDesY, nDesZ, 0.5, bCareless)
  Event.Create(Event.ObjectWinched, {
    uCargo,
    uHeli,
    "Detach"
  }, DeliveryComplete, {uHeli})
end

function DeliveryComplete(uHeli)
  Object.DetachCargoFromWinch(uHeli)
  self = {}
  MrxSupport.GoHome(self, uHeli)
end
