inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetSmokeColor("red")
  oDesignator:SetAATestLevel("basic")
  oDesignator:SetValidationFunction(nil)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxTankBuster")
  return oNewSupport
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(400, 150, -1, self.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(200, 60, -1, self.uOwner)
  self.uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 120, Strike, {self})
  Event.Create(Event.TimerRelative, {3}, MrxSupport.PlayAirstrikeVO, {
    self.uJet,
    {
      "Misha-None-Freeplay-Support-07",
      "Misha-None-Freeplay-Support-11",
      "Misha-None-Freeplay-Support-13",
      "Misha-None-Freeplay-Support-22",
      "Misha-None-Freeplay-Support-33"
    }
  })
end

function Strike(self)
  local uHero = Player.GetCharacter(self.uOwner)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(uHero)
  local tTargets = Pg.FastCollectTanks(nSpawnX, nSpawnY, nSpawnZ, 200)
  local nCount = 0
  for i, uTarget in ipairs(tTargets) do
    if not Vehicle.GetDriver(uTarget) or not Object.HasLabel(Vehicle.GetDriver(uTarget), "PMC") then
      Event.Create(Event.TimerRelative, {
        0.2 * nCount
      }, LaunchMissile, {self, uTarget})
      nCount = nCount + 1
    end
  end
end

function LaunchMissile(self, uTarget)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ = Object.GetPosition(uTarget)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Debug.Printf(tostring(nVectorX) .. " , " .. tostring(nVectorY) .. " , " .. tostring(nVectorZ))
  local nSpeedScale = 30
  local uBomb = Airstrike.SpawnTargettedOrdnance("Airstrike AT Missile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, uTarget, "impact", nil, self:GetOwner(), Object.Kill, {uTarget})
  BlipAircraft(uBomb, {
    255,
    0,
    0
  })
end

function _ValidateDropZone(fCallback, nX, nY, nZ, oSupport)
  local bDropZoneAdded = Ai.TestDropZone({
    Callback = fCallback,
    Location = {
      nX,
      nY,
      nZ
    },
    InnerRadius = 1,
    InnerHeightTolerance = 1,
    OuterRadius = 2,
    OuterHeightTolerance = 2,
    HeightMax = 5,
    SearchRadius = 12,
    Water = false
  })
  if not bDropZoneAdded then
    fCallback(false, "noland")
  end
end
