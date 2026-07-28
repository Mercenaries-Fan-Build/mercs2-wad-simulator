inherit("MrxSupportPickup")
import("MrxChiCon001Rescue")
import("MrxVoSequence")

function Create(oSelf, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, oSelf)
  oSelf.__index = oSelf
  oNewSupport.oTarget = oSelf.oTarget
  oNewSupport.sDeliveryVehicle = oSelf.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(oSelf.sDeliveryVehicle)
  oNewSupport:SetDesignator(MrxSupportDesignatorSmoke:Create())
  oNewSupport:SetOwner(uOwnerGuid)
  local oDesignator = oNewSupport:GetDesignator()
  oDesignator:SetValidationFunction(MrxSupportDesignator.ValidateLandingZone)
  oNewSupport:SetModuleName("MrxChiCon001Rescue")
  return oNewSupport
end

function AddSupport()
  for _, uPlayer in pairs(Player.GetAllPlayers()) do
    local oSupportMenu = MrxGui.GetWidgetByNameAndOwner("Support Menu", uPlayer)
    if oSupportMenu then
      oSupportMenu:AddItem({
        sName = "Rescue Copter",
        sIcon = "vehicles_helir_uh1",
        oSupport = MrxChiCon001Rescue:Create(uPlayer)
      })
    end
  end
end

function RemoveSupport()
  for _, uPlayer in ipairs(Player.GetAllPlayers()) do
    local oSupportMenu = MrxGui.GetWidgetByNameAndOwner("Support Menu", uPlayer)
    if oSupportMenu then
      oSupportMenu:RemoveItem("Rescue Copter")
    end
  end
end

function DesignationCallback(oSelf)
  Debug.Printf("Using the correct script, YAY")
  local nDesX, nDesY, nDesZ = oSelf.oDesignator:GetTarget()
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-150, nAltitude, -1, oSelf.uOwner)
  if nDesY and nTargetY < nDesY + nAltitude then
    nTargetY = nDesY + nAltitude
  end
  local uHeli = Pg.Spawn(oSelf.uDeliveryVehicle, nTargetX, nTargetY, nTargetZ, 0, false, true)
  if not uHeli then
    return
  end
  local nHeliX, nHeliY, nHeliZ = Object.GetPosition(uHeli)
  local nTargetX, nTargetY, nTargetZ = oSelf.oDesignator:GetTarget()
  local nOrientation = Math.GetXZHeading(nTargetX - nHeliX, nTargetY - nHeliY, nTargetZ - nHeliZ)
  Object.SetYaw(uHeli, nOrientation)
  MrxVoSequence.Start("Ewan-None-Freeplay-Support-28", nil, MrxVoSequence.knPriorityFreeplay)
  Event.Create(Event.TimerRelative, {2}, _WaitCallback, {oSelf, uHeli})
end

function _WaitCallback(oSelf, uHeli)
  local nX, nY, nZ = oSelf.oDesignator:GetTarget()
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(uHeli),
    Goal = "HeliLand",
    Location = {
      nX,
      nY,
      nZ
    },
    Priority = "hiPri",
    Force = true,
    Callback = _VehicleLanded,
    CallbackData = {oSelf, uHeli}
  })
end

function _VehicleLanded(oSelf, uHeli, uDriver, nState)
  Debug.Printf("<--> _VehicleLanded: " .. tostring(nState))
  if nState == 0 then
    MrxSupport.DenialMessage("abortnodrop")
    GoHome(oSelf, uHeli, uDriver)
    return
  end
  Ai.Role({
    AIGuid = uDriver,
    Role = "Idle",
    Priority = "hiPri"
  })
  local x, y, z = Object.GetPosition(uHeli)
  local tPrisoners = Pg.FastCollectHumans(x, y, z, 60, "Prisoner")
  local nPrisoners = table.getn(tPrisoners)
  Debug.Printf("There are " .. table.getn(tPrisoners) .. " prisoners")
  if nPrisoners and 1 <= nPrisoners then
    for i, uPrisoner in ipairs(tPrisoners) do
      Ai.Role({
        AIGuid = uPrisoner,
        Role = "Idle",
        Priority = "loPri"
      })
      a = Ai.Goal({
        AIGuid = uPrisoner,
        Goal = "Enter",
        Target = uHeli,
        Role = "Passenger",
        Priority = "hiPri"
      })
      Debug.Printf("Ai.Goal: " .. tostring(a))
    end
  end
  Event.Create(Event.ObjectInSeat, {
    "Prisoner",
    uHeli,
    "Any",
    "Enter"
  }, GoHome, {
    oSelf,
    uHeli,
    uDriver
  })
end
