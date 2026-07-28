inherit("MrxSupport")
import("MrxSupportManager")
import("MrxSupportDesignatorSmoke")
import("MrxUtil")
sDeliveryVehicle = "UH1 Transport (GR) (Full)"
nAltitude = 50
sModuleName = "MrxSoldierDelivery"
tVOOnTheWay = {
  Pmc = {
    "Ewan-None-Freeplay-Support-73",
    "Ewan-None-Freeplay-Support-38"
  },
  Allied = {
    "AlliedSoldier01.Troops.Incoming01",
    "AlliedSoldier01.Troops.Incoming02",
    "AlliedSoldier01.Troops.Incoming03"
  },
  China = {
    "ChinaSoldier01.Troops.Incoming01",
    "ChinaSoldier01.Troops.Incoming02",
    "ChinaSoldier01.Troops.Incoming03"
  },
  Pirate = {
    "Fiona.PirateCoverage.Reinforcements02"
  },
  Guerilla = {
    "GurSoldier01.Troops.Incoming01",
    "GurSoldier01.Troops.Incoming02",
    "GurSoldier01.Troops.Incoming03"
  },
  OC = {
    "OCSoldier01.Troops.Incoming01",
    "OCSoldier01.Troops.Incoming02",
    "OCSoldier01.Troops.Incoming03"
  }
}
oFinalDestination = "01_pmc_hq_lz_playerone"

function Create(self, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport.oTarget = self.oTarget
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.oFinalDestination = self.oFinalDestination
  oNewSupport.oUpdateEvent = nil
  oNewSupport:SetOwner(uOwnerGuid)
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetSmokeColor("blue")
  oDesignator:SetAATestLevel("none")
  oDesignator:SetValidationFunction(CheckForSoldiers)
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport.sModuleName = self.sModuleName
  oNewSupport:SetRecruit("Fiona")
  return oNewSupport
end

function DesignationCallback(self)
  _DesignatorCallback(self)
end

function SetDeliveryVehicle(self, sVehicleTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sVehicleTemplateName) then
    return
  end
  self.sDeliveryVehicle = sVehicleTemplateName
  self.uDeliveryVehicle = Pg.GetGuidByName(sVehicleTemplateName)
end

function SetFinalDestination(self, oFinalDestination)
  if "table" ~= type(self) then
    return
  end
  self.oFinalDestination = oFinalDestination
end

function _DesignatorCallback(self)
  if not Net.IsClient() then
    local nDesX, nDesY, nDesZ = self.oDesignator:GetTarget()
    local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-150, MrxSupport.GetSpawnHeight(), -1, self.uOwner, 300 + math.randi(120))
    if nDesY and nTargetY < nDesY + nAltitude then
      nTargetY = nDesY + nAltitude
    end
    local uHeli = Pg.Spawn(self.uDeliveryVehicle, nTargetX, nTargetY, nTargetZ, 0, false, true)
    if not uHeli then
      return
    end
    local nHeliX, nHeliY, nHeliZ = Object.GetPosition(uHeli)
    local nTargetX, nTargetY, nTargetZ = self.oDesignator:GetTarget()
    local nOrientation = Math.GetXZHeading(nTargetX - nHeliX, nTargetY - nHeliY, nTargetZ - nHeliZ)
    Object.SetYaw(uHeli, nOrientation)
    Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _WaitCallback, {
      self,
      uHeli,
      nTargetX,
      nTargetY,
      nTargetZ
    })
    MrxSupport.SetupDamageEvent(self, uHeli, false)
  end
end

function _WaitCallback(self, uHeli, nTargetX, nTargetY, nTargetZ)
  local sFaction = MrxUtil.GetFaction(uHeli)
  Debug.Printf("Playing VO Cue for " .. tostring(sFaction))
  if sFaction and tVOOnTheWay[sFaction] then
    MrxSupport.PlayRandomVOCue(tVOOnTheWay[sFaction])
  else
    Debug.Printf("No cue found for " .. tostring(tVOOnTheWay[sFaction]))
  end
  local uDriver = Vehicle.GetDriver(uHeli)
  self.LandGoal = Ai.Goal({
    AIGuid = Vehicle.GetDriver(uHeli),
    Goal = "HeliLand",
    Location = {
      nTargetX,
      nTargetY,
      nTargetZ
    },
    Priority = "hiPri",
    Callback = AllOut,
    CallbackData = {self, uHeli}
  })
  Debug.Printf("<--> Original command: " .. tostring(self.LandGoal))
  Object.AddLabel(uHeli, "Disposable")
end

function AllOut(self, uHeli, uDriver, nState)
  self.bSupportComplete = true
  if nState == 0 then
    MrxSupport.DenialMessage("abortnodrop")
    MrxSupport.GoHome(self, uHeli, uDriver)
    return
  end
  local tRiders = Vehicle.GetRiders(uHeli, "p")
  Ai.Deploy({
    Vehicle = uHeli,
    Role = "Passenger",
    Force = true,
    MaintainRotorSpeed = true,
    Callback = FollowTheLeader,
    CallbackData = {
      self,
      tRiders,
      uHeli,
      uDriver
    }
  })
end

function FollowTheLeader(self, tRiders, uHeli, uDriver)
  for i, uDude in ipairs(tRiders) do
    if Object.IsAlive(uDude) then
      Ai.Role({
        AIGuid = uDude,
        Role = "Follow",
        Target = Player.GetCharacter(self.uOwner),
        MinDistance = 10,
        MoveDistance = 12,
        MaxDistance = 50,
        Priority = "medPri"
      })
    end
  end
  MrxSupport.GoHome(self, uHeli, uDriver)
end

function CheckForSoldiers(fCallback, nX, nY, nZ, self)
  local sFaction = MrxUtil.GetFaction(self.uDeliveryVehicle)
  if sFaction then
    tRushers = Pg.FastCollectHumans(nX, nY, nZ, 80, sFaction)
  end
  tRushers = tRushers or {}
  print(tostring(table.getn(tRushers)) .. " found in area (" .. tostring(sFaction) .. ")")
  if table.getn(tRushers) < 8 then
    MrxSupportDesignator.ValidateLandingZone(fCallback, nX, nY, nZ, self)
  else
    fCallback(false, "toomanysoldiers" .. tostring(sFaction))
  end
end
