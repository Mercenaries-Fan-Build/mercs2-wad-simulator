inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetSmokeColor("red")
  oDesignator:SetAATestLevel("basic")
  oDesignator:SetValidationFunction(nil)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxCombatAirPatrol")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  return oNewSupport
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 100, -1, self.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(200, 100, -1, self.uOwner)
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY + 65, 100, Strike, {self})
  self.uJet = uJet
  Event.Create(Event.TimerRelative, {3}, MrxSupport.PlayAirstrikeVO, {
    uJet,
    {
      "Misha-None-Freeplay-Support-07",
      "Misha-None-Freeplay-Support-13",
      "Misha-None-Freeplay-Support-15",
      "Misha-None-Freeplay-Support-26"
    }
  })
end

function Strike(self)
  local uHero = Player.GetCharacter(self.uOwner)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(uHero)
  local oFilter = ObjectFilter.Create()
  local tTargets = Pg.FastCollectFlying(nSpawnX, nSpawnY, nSpawnZ, 200)
  Debug.Printf("Found " .. tostring(table.getn(tTargets)) .. " target(s)")
  local nCount = 0
  for i, uTarget in ipairs(tTargets) do
    if Vehicle.GetDriver(uTarget) and not Object.HasLabel(Vehicle.GetDriver(uTarget), "pmc") then
      Event.Create(Event.TimerRelative, {
        0.2 * nCount
      }, LaunchMissile, {self, uTarget})
      nCount = nCount + 1
      Debug.Printf("Launching missile...")
    end
  end
end

function LaunchMissile(self, uTarget)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ = Object.GetPosition(uTarget)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Debug.Printf("Position: " .. tostring(nSpawnX) .. ", " .. tostring(nSpawnY) .. ", " .. tostring(nSpawnZ))
  local uBomb = Airstrike.SpawnTargettedOrdnance("Airstrike AA Missile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, uTarget, "impact", nil, self:GetOwner())
  BlipAircraft(uBomb, {
    255,
    0,
    0
  })
end
