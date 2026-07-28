inherit("MrxSupport")
import("MrxSupportDesignatorSatellite")
sBomb = "Smart Bomb Projectile"
uBomb = Pg.GetGuidByName("Smart Bomb Projectile")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSatellite:Create()
  oDesignator:SetCost(self.nCost or 1000)
  Debug.Printf("Setting satellite cost (default)")
  oDesignator:SetMinigameSectors({
    {135, 225},
    {-45, 45}
  })
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetModuleName("MrxSatelliteGuidedBomb")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.sBomb = self.sBomb
  oNewSupport.uBomb = Pg.GetGuidByName(self.sBomb)
  return oNewSupport
end

function SetCost(self, nCost)
  self:GetDesignator():SetCost(nCost)
  self.nCost = nCost
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, 8, -1, self.uOwner)
  local nLaunchX, nLaunchY, nLaunchZ = Pg.FindPointFromCamera(-100, 80)
  local nTargetX, nTargetY, nTargetZ = self:GetDesignator():GetTarget()
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nLaunchX, nLaunchZ, nLaunchY, 200, DropBomb, {self})
  self.uJet = uJet
  MrxSupport.PlayAirstrikeVO(uJet, {
    "Misha-None-Freeplay-Support-01",
    "Misha-None-Freeplay-Support-10",
    "Misha-None-Freeplay-Support-21",
    "Misha-None-Freeplay-Support-24",
    "Misha-None-Freeplay-Support-28",
    "Misha-None-Freeplay-Support-32"
  })
end

function DropBomb(self)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ = self:GetDesignator():GetTarget()
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  self.uTarget = uGuid
  self.tBombs = {}
  self.tBombs[1] = Airstrike.SpawnOrdnance(self.uBomb, nSpawnX, nSpawnY, nSpawnZ, nVectorX * 110, nVectorY * 110, nVectorZ * 110, "impact", 2, self:GetOwner(), BombExplodes, {self, 1})
  BlipAircraft(self.tBombs[1], {
    255,
    0,
    0
  })
end

function BombExplodes(self, nIndex)
  local nBombX, nBombY, nBombZ = Object.GetPosition(self.tBombs[nIndex])
end

function FinalExplosion(self)
  local nBombX, nBombY, nBombZ = Object.GetPosition(self.uBomb)
end
